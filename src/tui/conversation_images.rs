//! Local conversation images. File reads, decoding, resizing, and protocol encoding stay off the
//! draw thread. Callers retain markdown fallback rows until an image is ready.
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::BufReader,
    ops::Range,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};

use ratatui::{
    layout::{Rect, Size},
    Frame,
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    sliced::{SignedPosition, SlicedImage, SlicedProtocol},
    Resize,
};

// Match the existing preview decoder/cache allocation budget. Encoded terminal data is additional;
// only one worker and one queued result can retain uncached images at a time.
const IMAGE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Key {
    path: PathBuf,
    available: Size,
}

/// A ready, shareable image with terminal encoding already prepared by the worker.
#[derive(Clone)]
pub struct ConversationImage {
    protocol: Arc<Mutex<EncodedImage>>,
    size: Size,
    decoded_bytes: usize,
}

struct EncodedImage {
    active: SlicedProtocol,
    standby: Option<SlicedProtocol>,
    pixels: Option<Arc<image::DynamicImage>>,
    rendered: bool,
    visible: bool,
    seen: bool,
    needs_retransmit: bool,
}

impl std::fmt::Debug for ConversationImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversationImage")
            .field("size", &self.size())
            .finish_non_exhaustive()
    }
}

impl ConversationImage {
    pub fn size(&self) -> Size {
        self.size
    }
}

struct Job {
    generation: u64,
    key: Key,
    pixels: Option<Arc<image::DynamicImage>>,
}

struct Completed {
    generation: u64,
    key: Key,
    result: Result<ConversationImage, String>,
}

/// Session-local cache. A missing picker disables images without changing markdown fallback text.
/// Dropping this owner disconnects its worker; no runtime or desktop integration is required.
pub struct ConversationImages {
    cwd: PathBuf,
    home: Option<PathBuf>,
    jobs: Option<mpsc::SyncSender<Job>>,
    completed: mpsc::Receiver<Completed>,
    generation: u64,
    ready: HashMap<Key, ConversationImage>,
    failed: HashMap<Key, String>,
    pending: HashSet<Key>,
    attempted: HashSet<Key>,
    recent: VecDeque<Key>,
    bytes: usize,
}

impl Default for ConversationImages {
    fn default() -> Self {
        Self::new(PathBuf::new(), None)
    }
}

impl std::fmt::Debug for ConversationImages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversationImages")
            .field("cwd", &self.cwd)
            .finish_non_exhaustive()
    }
}

impl ConversationImages {
    /// Reuse the preview's configured/detected picker rather than querying the terminal again.
    pub fn new(cwd: PathBuf, picker: Option<Picker>) -> Self {
        let (jobs, requests) = mpsc::sync_channel::<Job>(1);
        let (results, completed) = mpsc::sync_channel(1);
        let jobs = picker.and_then(|picker| {
            std::thread::Builder::new()
                .name("conversation-images".into())
                .spawn(move || {
                    while let Ok(job) = requests.recv() {
                        let result = catch_image_panic(|| match job.pixels {
                            Some(pixels) => {
                                encode_image(&picker, (*pixels).clone(), job.key.available)
                            }
                            None => load_image(&picker, &job.key),
                        });
                        if results
                            .send(Completed {
                                generation: job.generation,
                                key: job.key,
                                result,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                })
                .ok()
                .map(|_| jobs)
        });
        Self {
            cwd,
            home: directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_owned()),
            jobs,
            completed,
            generation: 0,
            ready: HashMap::new(),
            failed: HashMap::new(),
            pending: HashSet::new(),
            attempted: HashSet::new(),
            recent: VecDeque::new(),
            bytes: 0,
        }
    }

    /// Request an image without waiting. Repeated calls coalesce while loading; unsupported paths,
    /// disabled graphics, and failed decodes return None so the caller keeps the fallback row.
    pub fn image(&mut self, source: &str, available: Size) -> Option<ConversationImage> {
        self.poll();
        if available.width == 0 || available.height == 0 {
            return None;
        }
        let key = Key {
            path: local_path(source, &self.cwd, self.home.as_deref())?,
            // SlicedImage uses signed row coordinates internally.
            available: Size::new(available.width, available.height.min(i16::MAX as u16)),
        };
        if let Some(image) = self.ready.get(&key).cloned() {
            self.recent.retain(|old| old != &key);
            self.recent.push_back(key);
            return Some(image);
        }
        if self.failed.contains_key(&key) || self.attempted.contains(&key) {
            return None;
        }
        self.schedule(key, None);
        None
    }

    /// Layout lookup without speculative IO. Request missing images only for visible source rows.
    pub fn cached_image(&self, source: &str, available: Size) -> Option<ConversationImage> {
        let key = Key {
            path: local_path(source, &self.cwd, self.home.as_deref())?,
            available: Size::new(available.width, available.height.min(i16::MAX as u16)),
        };
        self.ready.get(&key).cloned()
    }

    /// Retry an evicted image only when its fallback row enters the viewport.
    pub fn request_visible(&mut self, source: &str, available: Size) {
        let Some(path) = local_path(source, &self.cwd, self.home.as_deref()) else {
            return;
        };
        let key = Key {
            path,
            available: Size::new(available.width, available.height.min(i16::MAX as u16)),
        };
        if available.width > 0
            && available.height > 0
            && !self.ready.contains_key(&key)
            && !self.failed.contains_key(&key)
        {
            self.schedule(key, None);
        }
    }

    fn schedule(&mut self, key: Key, pixels: Option<Arc<image::DynamicImage>>) {
        if self.pending.contains(&key) {
            return;
        }
        if let Some(jobs) = &self.jobs {
            match jobs.try_send(Job {
                generation: self.generation,
                key: key.clone(),
                pixels,
            }) {
                Ok(()) => {
                    self.attempted.insert(key.clone());
                    self.pending.insert(key);
                }
                Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => self.jobs = None,
            }
        }
    }

    /// Call after every terminal draw, even when the conversation is hidden. Rendering placements
    /// records visibility; this closes the frame and replenishes Kitty standbys off-thread.
    pub fn end_frame(&mut self) {
        let mut refresh = Vec::new();
        for (key, image) in &self.ready {
            let mut encoded = image.protocol.lock().expect("image state lock");
            encoded.visible = std::mem::take(&mut encoded.seen);
            if encoded.visible {
                self.recent.retain(|old| old != key);
                self.recent.push_back(key.clone());
            }
            if encoded.standby.is_none() && !self.failed.contains_key(key) {
                if let Some(pixels) = &encoded.pixels {
                    refresh.push((key.clone(), Arc::clone(pixels)));
                }
            }
        }
        for (key, pixels) in refresh {
            self.schedule(key, Some(pixels));
        }
    }

    /// Drain completed work. A true return value means cached conversation layout should refresh.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(done) = self.completed.try_recv() {
            if done.generation != self.generation {
                continue;
            }
            self.pending.remove(&done.key);
            changed = true;
            match done.result {
                Ok(image) => {
                    if let Some(cached) = self.ready.get(&done.key) {
                        let mut encoded = cached.protocol.lock().expect("image state lock");
                        let mut replacement = image.protocol.lock().expect("image state lock");
                        if encoded.needs_retransmit {
                            std::mem::swap(&mut encoded.active, &mut replacement.active);
                            encoded.rendered = false;
                            encoded.needs_retransmit = false;
                            encoded.standby = replacement.standby.take();
                        } else {
                            let standby = replacement
                                .standby
                                .take()
                                .expect("Kitty replacement standby");
                            encoded.standby =
                                Some(std::mem::replace(&mut replacement.active, standby));
                        }
                        continue;
                    }
                    while self.bytes.saturating_add(image.decoded_bytes) > IMAGE_BYTES {
                        let Some(old) = self.recent.pop_front() else {
                            break;
                        };
                        if let Some(evicted) = self.ready.remove(&old) {
                            self.bytes = self.bytes.saturating_sub(evicted.decoded_bytes);
                            self.attempted.insert(old);
                        }
                    }
                    self.bytes += image.decoded_bytes;
                    self.recent.push_back(done.key.clone());
                    self.ready.insert(done.key, image);
                }
                Err(error) => {
                    self.failed.insert(done.key, error);
                }
            }
        }
        changed
    }

    /// Explain a failed load while retaining the markdown's alt/path fallback text.
    pub fn load_error(&self, source: &str, available: Size) -> Option<&str> {
        let key = Key {
            path: local_path(source, &self.cwd, self.home.as_deref())?,
            available: Size::new(available.width, available.height.min(i16::MAX as u16)),
        };
        self.failed.get(&key).map(String::as_str)
    }

    /// Forget session images and failed loads. Late results from the old session are ignored.
    pub fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.ready.clear();
        self.failed.clear();
        self.pending.clear();
        self.attempted.clear();
        self.recent.clear();
        self.bytes = 0;
    }
}

/// Accept local filesystem references only, including local file URLs and explicit home paths.
/// No network requests, URL handlers, subprocesses, or desktop viewers are involved.
fn local_path(source: &str, cwd: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if source.is_empty() || source.chars().any(char::is_control) || source.starts_with("//") {
        return None;
    }
    let decoded;
    let source = if let Some(url) = source.strip_prefix("file://") {
        let path = url
            .strip_prefix("localhost/")
            .map(|p| format!("/{p}"))
            .or_else(|| url.starts_with('/').then(|| url.to_owned()))?;
        if path.contains(['?', '#']) {
            return None;
        }
        let mut bytes = Vec::new();
        let mut input = path.bytes();
        while let Some(byte) = input.next() {
            bytes.push(if byte == b'%' {
                let hi = (input.next()? as char).to_digit(16)?;
                let lo = (input.next()? as char).to_digit(16)?;
                (hi * 16 + lo) as u8
            } else {
                byte
            });
        }
        decoded = String::from_utf8(bytes).ok()?;
        if decoded.chars().any(char::is_control) || decoded.starts_with("//") {
            return None;
        }
        &decoded
    } else {
        // Reject URL schemes, including data: and remote schemes without //.
        if source.split('/').next()?.contains(':') || source.starts_with("\\\\") {
            return None;
        }
        source
    };
    if source == "~" {
        return home.map(Path::to_owned);
    }
    if let Some(rest) = source.strip_prefix("~/") {
        return home.map(|home| home.join(rest));
    }
    let path = Path::new(source);
    Some(if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    })
}

fn load_image(picker: &Picker, key: &Key) -> Result<ConversationImage, String> {
    let mut options = File::options();
    options.read(true);
    // Opening a FIFO must not strand the sole worker before metadata can reject it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(&key.path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("conversation images require a regular local file".into());
    }
    if metadata.len() > IMAGE_BYTES as u64 {
        return Err(format!(
            "conversation image file budget is {IMAGE_BYTES} bytes; requested {} bytes",
            metadata.len()
        ));
    }
    let mut reader = image::ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(IMAGE_BYTES as u64);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| {
        format!("conversation image decode failed (allocation budget {IMAGE_BYTES} bytes): {error}")
    })?;
    encode_image(picker, image, key.available)
}

fn encode_image(
    picker: &Picker,
    image: image::DynamicImage,
    available: Size,
) -> Result<ConversationImage, String> {
    let size = Resize::Fit(None).size_for(&image, picker.font_size(), available);
    let resized = Resize::Fit(None).resize(&image, picker.font_size(), size, None);
    // Reserve the resized RGBA footprint for each protocol, plus Kitty's retained refresh pixels.
    // Encoded protocol data is additional, as with the preview cache.
    let kitty = picker.protocol_type() == ProtocolType::Kitty;
    let decoded_bytes = (resized.width() as usize)
        .saturating_mul(resized.height() as usize)
        .saturating_mul(4)
        .saturating_mul(if kitty { 3 } else { 1 });
    if decoded_bytes > IMAGE_BYTES {
        return Err(format!("conversation image cache budget is {IMAGE_BYTES} bytes; requested {decoded_bytes} bytes"));
    }
    let pixels = kitty.then(|| Arc::new(resized.clone()));
    let standby = if kitty {
        Some(
            SlicedProtocol::new(picker, resized.clone(), Some(size))
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let protocol =
        SlicedProtocol::new(picker, resized, Some(size)).map_err(|error| error.to_string())?;
    Ok(ConversationImage {
        protocol: Arc::new(Mutex::new(EncodedImage {
            active: protocol,
            standby,
            pixels,
            rendered: false,
            visible: false,
            seen: false,
            needs_retransmit: false,
        })),
        size,
        decoded_bytes,
    })
}

fn catch_image_panic(
    load: impl FnOnce() -> Result<ConversationImage, String>,
) -> Result<ConversationImage, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(load)).unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("unknown panic");
        Err(format!("conversation image worker panicked: {detail}"))
    })
}

/// Image rows are relative to the same content origin as the associated text lines.
#[derive(Clone, Debug)]
pub struct ImagePlacement {
    pub image: ConversationImage,
    pub rows: Range<usize>,
}

impl ImagePlacement {
    /// Paint only rows inside the conversation viewport. Scrolling crops, never rescales, images.
    pub fn render(&self, frame: &mut Frame<'_>, viewport: Rect, scroll_row: usize) {
        let start = self.rows.start.max(scroll_row);
        let end = self
            .rows
            .end
            .min(scroll_row.saturating_add(usize::from(viewport.height)));
        if start >= end || viewport.width == 0 {
            return;
        }
        let area = Rect::new(
            viewport.x,
            viewport.y + (start - scroll_row) as u16,
            viewport.width,
            (end - start) as u16,
        );
        let skip = i16::try_from(start - self.rows.start).unwrap_or(i16::MAX);
        let mut encoded = self.image.protocol.lock().expect("image state lock");
        if !encoded.visible && !encoded.seen && encoded.rendered && encoded.pixels.is_some() {
            if let Some(standby) = encoded.standby.take() {
                encoded.active = standby;
                encoded.rendered = false;
            } else {
                encoded.needs_retransmit = true;
            }
        }
        encoded.seen = true;
        frame.render_widget(
            SlicedImage::new(&encoded.active, SignedPosition { x: 0, y: -skip }),
            area,
        );
        encoded.rendered = true;
    }
}

#[cfg(test)]
#[path = "conversation_images_tests.rs"]
mod tests;

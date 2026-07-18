use image::ImageReader;
use ratatui::{
    layout::{Rect, Size},
    Frame,
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, ResizeEncodeRender, StatefulImage,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
};

/// Bounds source pixels retained by terminal protocols. Protocol-specific encoded allocations are
/// opaque, but remain proportional to these bounded source images.
const MAX_DECODED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_DECODED_CACHE_BUDGET: usize = 256 * 1024 * 1024;
// Keep speculative warm-up shallow so a newly selected slide can enter the worker quickly.
const WORK_QUEUE_CAPACITY: usize = 4;
const RESULT_QUEUE_CAPACITY: usize = 2;
const ENCODE_WORKERS: usize = 2;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    path: PathBuf,
    cells: Size,
}

struct EncodeJob {
    generation: u64,
    key: CacheKey,
}

struct EncodedProtocol {
    protocol: StatefulProtocol,
    decoded_bytes: usize,
}

struct EncodeResult {
    generation: u64,
    key: CacheKey,
    result: Result<EncodedProtocol, String>,
}

/// Result of trying to draw a terminal image in this frame.
#[derive(Debug, Eq, PartialEq)]
pub enum ImageRenderStatus {
    Ready,
    Loading,
    Error(String),
}

/// Terminal-ready image protocols shared by the workspace preview and fullscreen presentation.
/// All file IO, decoding, resizing, and terminal encoding happens on the worker thread.
pub struct PreviewImage {
    jobs: SyncSender<EncodeJob>,
    completed: Receiver<EncodeResult>,
    generation: u64,
    current_generation: Arc<AtomicU64>,
    deck_paths: Vec<PathBuf>,
    protocols: HashMap<CacheKey, EncodedProtocol>,
    errors: HashMap<CacheKey, String>,
    pending: HashSet<CacheKey>,
    attempted: HashSet<CacheKey>,
    recency: VecDeque<CacheKey>,
    decoded_bytes: usize,
    decoded_cache_budget: usize,
    worker_available: bool,
}

impl PreviewImage {
    pub fn detect(configured_protocol: &str) -> Self {
        Self::detect_with_budget(configured_protocol, DEFAULT_DECODED_CACHE_BUDGET)
    }

    fn detect_with_budget(configured_protocol: &str, decoded_cache_budget: usize) -> Self {
        assert!(decoded_cache_budget > 0, "preview cache must be non-zero");
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        if let Some(protocol) = configured_protocol_type(configured_protocol) {
            picker.set_protocol_type(protocol);
        }

        let (job_sender, job_receiver) = mpsc::sync_channel(WORK_QUEUE_CAPACITY);
        let (result_sender, result_receiver) = mpsc::sync_channel(RESULT_QUEUE_CAPACITY);
        let current_generation = Arc::new(AtomicU64::new(0));
        let job_receiver = Arc::new(Mutex::new(job_receiver));
        let mut worker_count = 0;
        for index in 0..ENCODE_WORKERS {
            let picker = picker.clone();
            let job_receiver = Arc::clone(&job_receiver);
            let result_sender = result_sender.clone();
            let worker_generation = Arc::clone(&current_generation);
            if std::thread::Builder::new()
                .name(format!("slide-preview-encode-{index}"))
                .spawn(move || {
                    encode_worker(picker, job_receiver, result_sender, worker_generation)
                })
                .is_ok()
            {
                worker_count += 1;
            }
        }
        let worker_available = worker_count > 0;

        Self {
            jobs: job_sender,
            completed: result_receiver,
            generation: 0,
            current_generation,
            deck_paths: Vec::new(),
            protocols: HashMap::new(),
            errors: HashMap::new(),
            pending: HashSet::new(),
            attempted: HashSet::new(),
            recency: VecDeque::new(),
            decoded_bytes: 0,
            decoded_cache_budget,
            worker_available,
        }
    }

    /// Replace the deck whose terminal encodings should be warmed. Actual work starts as soon as
    /// either view supplies its render cell size, with the active slide queued before its peers.
    pub fn preload_deck(&mut self, paths: Vec<PathBuf>) {
        self.generation = self.generation.wrapping_add(1);
        self.current_generation
            .store(self.generation, Ordering::Release);
        self.deck_paths = paths;
        self.protocols.clear();
        self.errors.clear();
        self.pending.clear();
        self.attempted.clear();
        self.recency.clear();
        self.decoded_bytes = 0;
        self.collect_completed();
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, path: &Path) -> ImageRenderStatus {
        self.collect_completed();
        if area.width == 0 || area.height == 0 {
            return ImageRenderStatus::Loading;
        }

        let key = CacheKey {
            path: path.to_path_buf(),
            cells: area.as_size(),
        };
        if self.protocols.contains_key(&key) {
            self.mark_recent(&key);
            let protocol = &mut self
                .protocols
                .get_mut(&key)
                .expect("a ready terminal protocol was just found")
                .protocol;
            let resize = Resize::Scale(None);
            let render_area =
                centered_image_area(area, protocol.size_for(resize.clone(), area.as_size()));
            if render_area.width > 0 && render_area.height > 0 {
                frame.render_stateful_widget(
                    StatefulImage::new().resize(resize),
                    render_area,
                    protocol,
                );
            }
            return ImageRenderStatus::Ready;
        }
        if let Some(error) = self.errors.get(&key) {
            return ImageRenderStatus::Error(error.clone());
        }
        if !self.worker_available {
            return ImageRenderStatus::Error("terminal image worker could not start".into());
        }

        self.schedule_key(
            CacheKey {
                path: path.to_path_buf(),
                cells: area.as_size(),
            },
            true,
        );
        ImageRenderStatus::Loading
    }

    /// Warm every slide for each supplied view size without rescheduling entries evicted by the
    /// memory budget. The active slide is first, followed by forward and backward neighbors.
    pub fn warm_for_sizes(&mut self, active: &Path, sizes: &[Size]) {
        self.collect_completed();
        let mut ordered = Vec::with_capacity(self.deck_paths.len().max(1));
        ordered.push(active.to_path_buf());
        if let Some(active_index) = self.deck_paths.iter().position(|path| path == active) {
            for distance in 1..self.deck_paths.len() {
                if let Some(path) = self.deck_paths.get(active_index + distance) {
                    ordered.push(path.clone());
                }
                if let Some(path) = active_index
                    .checked_sub(distance)
                    .and_then(|index| self.deck_paths.get(index))
                {
                    ordered.push(path.clone());
                }
            }
        } else {
            ordered.extend(self.deck_paths.iter().cloned());
        }

        for path in ordered {
            for &cells in sizes {
                if cells.width == 0 || cells.height == 0 {
                    continue;
                }
                if !self.schedule_key(
                    CacheKey {
                        path: path.clone(),
                        cells,
                    },
                    false,
                ) {
                    return;
                }
            }
        }
    }

    fn schedule_key(&mut self, key: CacheKey, allow_evicted: bool) -> bool {
        if self.protocols.contains_key(&key)
            || self.errors.contains_key(&key)
            || self.pending.contains(&key)
            || (!allow_evicted && self.attempted.contains(&key))
        {
            return true;
        }
        match self.jobs.try_send(EncodeJob {
            generation: self.generation,
            key: key.clone(),
        }) {
            Ok(()) => {
                self.attempted.insert(key.clone());
                self.pending.insert(key);
                true
            }
            Err(TrySendError::Full(_)) => false,
            Err(TrySendError::Disconnected(_)) => {
                self.worker_available = false;
                false
            }
        }
    }

    fn collect_completed(&mut self) {
        loop {
            match self.completed.try_recv() {
                Ok(completed) => {
                    if completed.generation != self.generation {
                        continue;
                    }
                    self.pending.remove(&completed.key);
                    match completed.result {
                        Ok(encoded) => self.insert(completed.key, encoded),
                        Err(error) => {
                            self.errors.insert(completed.key, error);
                        }
                    }
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.worker_available = false;
                    return;
                }
            }
        }
    }

    fn insert(&mut self, key: CacheKey, encoded: EncodedProtocol) {
        while !self.protocols.is_empty()
            && self.decoded_bytes.saturating_add(encoded.decoded_bytes) > self.decoded_cache_budget
        {
            self.evict_oldest();
        }
        self.decoded_bytes = self.decoded_bytes.saturating_add(encoded.decoded_bytes);
        self.protocols.insert(key.clone(), encoded);
        self.recency.push_back(key);
    }

    fn evict_oldest(&mut self) {
        let Some(key) = self.recency.pop_front() else {
            return;
        };
        if let Some(encoded) = self.protocols.remove(&key) {
            self.decoded_bytes = self.decoded_bytes.saturating_sub(encoded.decoded_bytes);
        }
    }

    fn mark_recent(&mut self, key: &CacheKey) {
        if let Some(position) = self.recency.iter().position(|cached| cached == key) {
            self.recency.remove(position);
        }
        self.recency.push_back(key.clone());
    }
}

fn encode_worker(
    picker: Picker,
    jobs: Arc<Mutex<Receiver<EncodeJob>>>,
    completed: SyncSender<EncodeResult>,
    current_generation: Arc<AtomicU64>,
) {
    loop {
        let job = match jobs.lock() {
            Ok(jobs) => jobs.recv(),
            Err(_) => return,
        };
        let Ok(job) = job else {
            return;
        };
        if job.generation != current_generation.load(Ordering::Acquire) {
            continue;
        }
        let result = decode_and_encode(&picker, &job.key);
        if job.generation != current_generation.load(Ordering::Acquire) {
            continue;
        }
        if completed
            .send(EncodeResult {
                generation: job.generation,
                key: job.key,
                result,
            })
            .is_err()
        {
            return;
        }
    }
}

fn decode_and_encode(picker: &Picker, key: &CacheKey) -> Result<EncodedProtocol, String> {
    let mut reader = ImageReader::open(&key.path)
        .map_err(|error| format!("could not open image: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("could not identify image: {error}"))?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODED_IMAGE_BYTES);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| format!("could not decode image: {error}"))?;
    let decoded_bytes = image.as_bytes().len();
    let mut protocol = picker.new_resize_protocol(image);
    let resize = Resize::Scale(None);
    let fitted = protocol.size_for(resize.clone(), key.cells);
    protocol.resize_encode(&resize, fitted);
    if let Some(result) = protocol.last_encoding_result() {
        result.map_err(|error| format!("could not encode terminal image: {error}"))?;
    }
    Ok(EncodedProtocol {
        protocol,
        decoded_bytes,
    })
}

/// Place the fitted image rect in the center of `area`.
fn centered_image_area(area: Rect, fitted: Size) -> Rect {
    let width = fitted.width.min(area.width);
    let height = fitted.height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn configured_protocol_type(configured: &str) -> Option<ProtocolType> {
    match configured.trim().to_ascii_lowercase().as_str() {
        "auto" | "" => None,
        "kitty" => Some(ProtocolType::Kitty),
        "sixel" => Some(ProtocolType::Sixel),
        "iterm2" => Some(ProtocolType::Iterm2),
        "halfblocks" => Some(ProtocolType::Halfblocks),
        _ => None,
    }
}

#[cfg(test)]
#[path = "preview_image_tests.rs"]
mod tests;

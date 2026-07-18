use image::ImageReader;
use ratatui::{
    layout::{Rect, Size},
    Frame,
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    thread::{ResizeRequest, ResizeResponse, ThreadProtocol},
    Resize, StatefulImage,
};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

/// Background work for terminal image decode and resize/encode.
pub enum PreviewWorkerEvent {
    ImageLoaded {
        path: PathBuf,
        protocol: StatefulProtocol,
    },
    ImageLoadFailed {
        path: PathBuf,
        error: String,
    },
    ImageResized(ResizeResponse),
}

/// Terminal image protocol state for the currently visible rendered slide.
///
/// Decode and resize/encode run off the UI thread so interaction stays responsive.
pub struct PreviewImage {
    picker: Picker,
    loaded_path: Option<PathBuf>,
    loading_path: Option<PathBuf>,
    last_error: Option<String>,
    protocol: ThreadProtocol,
}

impl PreviewImage {
    pub fn detect(
        configured_protocol: &str,
        worker_events: mpsc::UnboundedSender<PreviewWorkerEvent>,
    ) -> Self {
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        if let Some(protocol) = configured_protocol_type(configured_protocol) {
            picker.set_protocol_type(protocol);
        }

        let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<ResizeRequest>();
        let resize_events = worker_events;
        tokio::spawn(async move {
            while let Some(request) = resize_rx.recv().await {
                let result = tokio::task::spawn_blocking(move || request.resize_encode()).await;
                if let Ok(Ok(response)) = result {
                    let _ = resize_events.send(PreviewWorkerEvent::ImageResized(response));
                }
            }
        });

        Self {
            picker,
            loaded_path: None,
            loading_path: None,
            last_error: None,
            protocol: ThreadProtocol::new(resize_tx, None),
        }
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Request decode of `path` if it is not already loaded or in flight.
    pub fn request_path(
        &mut self,
        path: &Path,
        worker_events: &mpsc::UnboundedSender<PreviewWorkerEvent>,
    ) {
        if self.loaded_path.as_deref() == Some(path) || self.loading_path.as_deref() == Some(path)
        {
            return;
        }

        // Drop the previous image so slide navigation never flashes the wrong slide.
        self.protocol.empty_protocol();
        self.loaded_path = None;
        self.last_error = None;
        self.loading_path = Some(path.to_path_buf());

        let path = path.to_path_buf();
        let picker = self.picker.clone();
        let worker_events = worker_events.clone();
        tokio::task::spawn_blocking(move || {
            let result = (|| -> Result<StatefulProtocol, String> {
                let image = ImageReader::open(&path)
                    .map_err(|error| error.to_string())?
                    .with_guessed_format()
                    .map_err(|error| error.to_string())?
                    .decode()
                    .map_err(|error| error.to_string())?;
                Ok(picker.new_resize_protocol(image))
            })();
            let event = match result {
                Ok(protocol) => PreviewWorkerEvent::ImageLoaded { path, protocol },
                Err(error) => PreviewWorkerEvent::ImageLoadFailed { path, error },
            };
            let _ = worker_events.send(event);
        });
    }

    /// Apply a completed background image job. Returns true when UI state changed.
    pub fn apply_worker_event(&mut self, event: PreviewWorkerEvent) -> bool {
        match event {
            PreviewWorkerEvent::ImageLoaded { path, protocol } => {
                if self.loading_path.as_ref() != Some(&path) {
                    return false;
                }
                self.loading_path = None;
                self.loaded_path = Some(path);
                self.last_error = None;
                self.protocol.replace_protocol(protocol);
                true
            }
            PreviewWorkerEvent::ImageLoadFailed { path, error } => {
                if self.loading_path.as_ref() != Some(&path) {
                    return false;
                }
                self.loading_path = None;
                self.last_error = Some(error);
                true
            }
            PreviewWorkerEvent::ImageResized(response) => {
                self.protocol.update_resized_protocol(response)
            }
        }
    }

    /// Paint the already-decoded protocol. Never performs disk or encode work.
    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, path: &Path) {
        if self.loaded_path.as_deref() != Some(path) {
            return;
        }
        // Scale fills the available area (Fit only downscales to natural size, which is often
        // undersized when pixel geometry queries fail inside multiplexers like herdr).
        let resize = Resize::Scale(None);
        let Some(fitted) = self.protocol.size_for(resize.clone(), area.as_size()) else {
            return;
        };
        let render_area = centered_image_area(area, fitted);
        if render_area.width > 0 && render_area.height > 0 {
            frame.render_stateful_widget(
                StatefulImage::new().resize(resize),
                render_area,
                &mut self.protocol,
            );
        }
    }
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

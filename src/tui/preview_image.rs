use image::ImageReader;
use ratatui::{
    layout::{Rect, Size},
    Frame,
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, StatefulImage,
};
use std::path::{Path, PathBuf};

/// Terminal image protocol state for the currently visible rendered slide.
pub struct PreviewImage {
    picker: Picker,
    loaded_path: Option<PathBuf>,
    protocol: Option<StatefulProtocol>,
}

impl PreviewImage {
    pub fn detect(configured_protocol: &str) -> Self {
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        if let Some(protocol) = configured_protocol_type(configured_protocol) {
            picker.set_protocol_type(protocol);
        }
        Self {
            picker,
            loaded_path: None,
            protocol: None,
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, path: &Path) -> anyhow::Result<()> {
        if self.loaded_path.as_deref() != Some(path) {
            self.loaded_path = None;
            self.protocol = None;
            let image = ImageReader::open(path)?.with_guessed_format()?.decode()?;
            self.protocol = Some(self.picker.new_resize_protocol(image));
            self.loaded_path = Some(path.to_path_buf());
        }
        if let Some(protocol) = &mut self.protocol {
            // Scale fills the available area (Fit only downscales to natural size, which is often
            // undersized when pixel geometry queries fail inside multiplexers like herdr).
            let resize = Resize::Scale(None);
            let render_area = centered_image_area(area, protocol.size_for(resize.clone(), area.as_size()));
            if render_area.width > 0 && render_area.height > 0 {
                frame.render_stateful_widget(
                    StatefulImage::new().resize(resize),
                    render_area,
                    protocol,
                );
            }
        }
        Ok(())
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

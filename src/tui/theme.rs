//! Shared visual vocabulary for the terminal workspace.

use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::Rgb(92, 166, 255);
pub const ACCENT_SOFT: Color = Color::Rgb(35, 57, 82);
pub const TEXT: Color = Color::Rgb(226, 232, 240);
pub const MUTED: Color = Color::Rgb(148, 163, 184);
pub const SUBTLE: Color = Color::Rgb(71, 85, 105);
pub const SURFACE: Color = Color::Rgb(15, 23, 35);
pub const SURFACE_RAISED: Color = Color::Rgb(24, 34, 48);
pub const SUCCESS: Color = Color::Rgb(74, 222, 128);
pub const WARNING: Color = Color::Rgb(250, 204, 21);
pub const DANGER: Color = Color::Rgb(248, 113, 113);

pub fn panel_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(SUBTLE)
    }
}

pub fn panel_title(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(TEXT)
            .bg(ACCENT_SOFT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED)
    }
}

pub fn keycap() -> Style {
    Style::default()
        .fg(TEXT)
        .bg(SURFACE_RAISED)
        .add_modifier(Modifier::BOLD)
}

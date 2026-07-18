//! Terminal-adaptive visual vocabulary for the workspace.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

#[path = "theme_terminal_palette.rs"]
mod terminal_palette;

use terminal_palette::{AnsiColor, ResolvedColor, TerminalPalette};

const NEUTRAL_BACKGROUND_ALPHA: f32 = 0.10;
const STATE_BACKGROUND_ALPHA: f32 = 0.16;
const ACCENT_BACKGROUND_ALPHA: f32 = 0.18;

static TERMINAL_PALETTE: OnceLock<TerminalPalette> = OnceLock::new();

pub const ACCENT: Color = Color::Cyan;
pub const TEXT: Color = Color::Reset;
pub const MUTED: Color = Color::DarkGray;
pub const SUBTLE: Color = Color::DarkGray;
pub const SURFACE: Color = Color::Reset;
pub const SUCCESS: Color = Color::Green;
pub const WARNING: Color = Color::Yellow;
pub const DANGER: Color = Color::Red;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockColor {
    color: Color,
    use_dark_foreground: bool,
}

impl BlockColor {
    const fn fallback(color: Color) -> Self {
        Self {
            color,
            use_dark_foreground: false,
        }
    }
}

impl From<ResolvedColor> for BlockColor {
    fn from(color: ResolvedColor) -> Self {
        Self {
            color: color.color,
            use_dark_foreground: color.use_dark_foreground,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Palette {
    neutral_background: BlockColor,
    accent_background: BlockColor,
    success_background: BlockColor,
    warning_background: BlockColor,
    danger_background: BlockColor,
}

impl Palette {
    fn current() -> Self {
        let terminal = TERMINAL_PALETTE.get();
        Self {
            neutral_background: blended_or_fallback(
                terminal,
                AnsiColor::Gray,
                NEUTRAL_BACKGROUND_ALPHA,
                Color::DarkGray,
            ),
            accent_background: blended_or_fallback(
                terminal,
                AnsiColor::Cyan,
                ACCENT_BACKGROUND_ALPHA,
                Color::Blue,
            ),
            success_background: blended_or_fallback(
                terminal,
                AnsiColor::Green,
                STATE_BACKGROUND_ALPHA,
                Color::Green,
            ),
            warning_background: blended_or_fallback(
                terminal,
                AnsiColor::Yellow,
                STATE_BACKGROUND_ALPHA,
                Color::Yellow,
            ),
            danger_background: blended_or_fallback(
                terminal,
                AnsiColor::Red,
                STATE_BACKGROUND_ALPHA,
                Color::Red,
            ),
        }
    }
}

pub fn initialize_from_terminal() {
    if let Some(palette) = terminal_palette::query() {
        let _ = TERMINAL_PALETTE.set(palette);
    }
}

pub fn panel_title() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn assistant_message() -> Style {
    Style::default()
}

pub fn user_message() -> Style {
    block_style(Palette::current().neutral_background)
}

pub fn system_message() -> Style {
    block_style(Palette::current().warning_background)
}

pub fn tool_proposed() -> Style {
    block_style(Palette::current().neutral_background).add_modifier(Modifier::BOLD)
}

pub fn tool_running() -> Style {
    block_style(Palette::current().warning_background).add_modifier(Modifier::BOLD)
}

pub fn tool_succeeded() -> Style {
    block_style(Palette::current().success_background).add_modifier(Modifier::BOLD)
}

pub fn tool_failed() -> Style {
    block_style(Palette::current().danger_background).add_modifier(Modifier::BOLD)
}

pub fn accent_block() -> Style {
    block_style(Palette::current().accent_background)
}

pub fn hover_block() -> Style {
    block_style(Palette::current().neutral_background)
}

pub fn keycap() -> Style {
    block_style(Palette::current().neutral_background).add_modifier(Modifier::BOLD)
}

fn block_style(background: BlockColor) -> Style {
    Style::default()
        .fg(if background.use_dark_foreground {
            Color::Black
        } else {
            Color::White
        })
        .bg(background.color)
}

fn blended_or_fallback(
    terminal: Option<&TerminalPalette>,
    color: AnsiColor,
    alpha: f32,
    fallback: Color,
) -> BlockColor {
    terminal
        .and_then(|palette| palette.blended_background(color, alpha))
        .map(BlockColor::from)
        .unwrap_or_else(|| BlockColor::fallback(fallback))
}

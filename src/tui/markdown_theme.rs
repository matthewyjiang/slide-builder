//! Rho renderer style adapter using the workspace's terminal-derived palette.
use ratatui::style::{Color, Modifier, Style};

use super::{markdown::HeadingLevel, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyntaxRole {
    Function,
    Type,
    Comment,
    String,
    Constant,
    Keyword,
}

pub(super) struct Theme;

impl Theme {
    pub(super) fn text() -> Style {
        theme::assistant_message()
    }

    pub(super) fn dim() -> Style {
        Self::text().fg(theme::MUTED)
    }

    pub(super) fn code_text() -> Style {
        Self::text()
    }

    pub(super) fn markdown_bold() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    pub(super) fn markdown_italic() -> Style {
        Style::default().add_modifier(Modifier::ITALIC)
    }

    pub(super) fn markdown_inline_code() -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }

    pub(super) fn markdown_link() -> Style {
        Style::default().add_modifier(Modifier::UNDERLINED)
    }

    pub(super) fn markdown_heading(level: HeadingLevel) -> Style {
        match level {
            HeadingLevel::H1 => theme::panel_title().add_modifier(Modifier::UNDERLINED),
            HeadingLevel::H2 => theme::panel_title(),
            HeadingLevel::H3 => theme::panel_title().add_modifier(Modifier::ITALIC),
            HeadingLevel::H4 => Self::text().add_modifier(Modifier::UNDERLINED),
            HeadingLevel::H5 => Self::text().add_modifier(Modifier::ITALIC),
            HeadingLevel::H6 => Self::dim().add_modifier(Modifier::ITALIC),
        }
    }

    pub(super) fn syntax(role: SyntaxRole) -> Style {
        let color = match role {
            SyntaxRole::Function => Color::Blue,
            SyntaxRole::Type => Color::Blue,
            SyntaxRole::Comment => theme::MUTED,
            SyntaxRole::String => theme::SUCCESS,
            SyntaxRole::Constant => theme::WARNING,
            SyntaxRole::Keyword => Color::Magenta,
        };
        Style::default().fg(color)
    }
}

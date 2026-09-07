//! Recent saved conversations, in the order supplied by session storage.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::popup;
use crate::tui::theme;

mod manager;
pub use manager::{SessionAction, SessionView};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionPickerMode {
    Resume,
    Manage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPickerEntry {
    pub id: String,
    pub name: String,
    pub deck: String,
    pub model: String,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPickerState {
    pub mode: SessionPickerMode,
    pub view: SessionView,
    pub entries: Vec<SessionPickerEntry>,
    pub filter: String,
    /// Index in the filtered list.
    pub selected: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionPickerEvent {
    None,
    Cancel,
    Selected(String),
    Rename { id: String, name: String },
    Delete(String),
}

impl SessionPickerState {
    pub fn new(entries: Vec<SessionPickerEntry>) -> Self {
        Self {
            mode: SessionPickerMode::Resume,
            view: SessionView::List,
            entries,
            filter: String::new(),
            selected: 0,
            error: None,
        }
    }

    pub fn visible(&self) -> Vec<&SessionPickerEntry> {
        let query = self.filter.to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                [&entry.id, &entry.name, &entry.deck, &entry.model]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&query))
            })
            .collect()
    }

    pub fn paste(&mut self, text: &str) {
        let target = match &mut self.view {
            SessionView::List => {
                self.selected = 0;
                &mut self.filter
            }
            SessionView::Rename { name, .. } => name,
            SessionView::Actions { .. } | SessionView::ConfirmDelete { .. } => return,
        };
        target.extend(text.chars().filter(|c| !c.is_control()));
        self.error = None;
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> SessionPickerEvent {
        if self.view != SessionView::List {
            return self.handle_manager_key(key);
        }
        match key.code {
            KeyCode::Esc => return SessionPickerEvent::Cancel,
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.visible().len().saturating_sub(1))
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.visible().len().saturating_sub(1),
            KeyCode::Enter => {
                if let Some(entry) = self.visible().get(self.selected) {
                    let entry = (*entry).clone();
                    match self.mode {
                        SessionPickerMode::Resume => return SessionPickerEvent::Selected(entry.id),
                        SessionPickerMode::Manage => {
                            self.error = None;
                            self.view = SessionView::Actions {
                                entry,
                                selected: SessionAction::Resume,
                            };
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
            }
            KeyCode::Char(c)
                if !c.is_control()
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.filter.push(c);
                self.selected = 0;
            }
            _ => {}
        }
        SessionPickerEvent::None
    }
}

pub fn render(frame: &mut Frame<'_>, state: &SessionPickerState) {
    if state.view != SessionView::List {
        manager::render(frame, state);
        return;
    }
    // Match the existing model picker's dimensions; the terminal remains the bound.
    let area = popup(frame, 76, 20);
    let (title, full_help, short_help) = match state.mode {
        SessionPickerMode::Resume => (
            " Resume session ",
            "type to filter  ↑↓ choose  Enter resume  Esc close",
            "↑↓  Enter resume  Esc close",
        ),
        SessionPickerMode::Manage => (
            " Saved sessions ",
            "type to filter  ↑↓ choose  Enter manage  Esc close",
            "↑↓  Enter manage  Esc close",
        ),
    };
    let help = if full_help.width() <= usize::from(area.width.saturating_sub(2)) {
        full_help
    } else {
        short_help
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let error = state.error.as_ref().map(|error| format!("Notice: {error}"));
    let error_rows = crate::tui::conversation_entry::plain_rows(
        error.as_deref().unwrap_or_default(),
        usize::from(inner.width),
        Style::default().fg(theme::WARNING),
    );
    let error_height = if error.is_some() {
        error_rows
            .len()
            .min(usize::from(inner.height.saturating_sub(3))) as u16
    } else {
        0
    };
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(error_height),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(theme::MUTED)),
            Span::styled(
                if state.filter.is_empty() {
                    "type to search"
                } else {
                    &state.filter
                },
                Style::default().fg(theme::TEXT),
            ),
        ])),
        rows[0],
    );
    frame.render_widget(Paragraph::new(error_rows), rows[1]);
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(theme::MUTED)),
        rows[3],
    );
    let visible = state.visible();
    if visible.is_empty() {
        let message = if state.entries.is_empty() {
            "No saved sessions yet."
        } else {
            "No sessions match this filter."
        };
        frame.render_widget(
            Paragraph::new(message)
                .style(Style::default().fg(theme::MUTED))
                .wrap(Wrap { trim: false }),
            rows[2],
        );
        return;
    }
    // The list reserves two columns for the selection marker on every detail row.
    let detail_width = usize::from(rows[2].width.saturating_sub(2));
    let items = visible.iter().map(|entry| {
        let mut name = Vec::new();
        // Put the current badge first so long names cannot hide it on narrow terminals.
        if entry.current {
            name.push(Span::styled(
                "current · ",
                Style::default().fg(theme::SUCCESS),
            ));
        }
        name.push(Span::styled(
            entry.name.as_str(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ));
        ListItem::new(vec![
            Line::from(name),
            Line::styled(
                detail_suffix(&entry.deck, detail_width),
                Style::default().fg(theme::MUTED),
            ),
            Line::styled(
                detail_suffix(&entry.model, detail_width),
                Style::default().fg(theme::MUTED),
            ),
        ])
    });
    let mut selection = ListState::default().with_selected(Some(state.selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("› ")
            .highlight_style(theme::accent_block()),
        rows[2],
        &mut selection,
    );
}

/// Keep the filename or model identifier visible when its prefix cannot fit.
fn detail_suffix(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut used = 1; // Reserve the ellipsis cell.
    let mut start = text.len();
    for (index, grapheme) in text.grapheme_indices(true).rev() {
        used += grapheme.width();
        if used > width {
            break;
        }
        start = index;
    }
    format!("…{}", &text[start..])
}

#[cfg(test)]
#[path = "session_picker_tests.rs"]
mod tests;

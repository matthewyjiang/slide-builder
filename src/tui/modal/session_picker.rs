//! Recent saved conversations, in the order supplied by session storage.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use super::popup;
use crate::tui::theme;

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
}

impl SessionPickerState {
    pub fn new(entries: Vec<SessionPickerEntry>) -> Self {
        Self {
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

    pub fn handle_key(&mut self, key: KeyEvent) -> SessionPickerEvent {
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
                    return SessionPickerEvent::Selected(entry.id.clone());
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
    // Match the existing model picker's dimensions; the terminal remains the bound.
    let area = popup(frame, 76, 20);
    let help = if area.width >= 60 {
        " type to filter  ↑↓ choose  Enter resume  Esc close "
    } else {
        " ↑↓  Enter resume  Esc close "
    };
    let block = Block::default()
        .title(" Saved sessions ")
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(Line::from(help).right_aligned())
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
        Constraint::Min(1),
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
                format!("{} · {}", entry.deck, entry.model),
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

#[cfg(test)]
#[path = "session_picker_tests.rs"]
mod tests;

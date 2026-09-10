//! `/model` picker: a filterable list of the models slide-builder can run
//! with its stored credentials. Selection is applied live; no restart needed.
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::{models::AvailableModel, tui::theme};

use super::popup;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPickerState {
    pub models: Vec<AvailableModel>,
    /// Index into `models` for the currently active model, if listed.
    pub current: Option<usize>,
    pub filter: String,
    /// Index into the filtered view, not into `models`.
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelPickerEvent {
    None,
    Cancel,
    Selected(AvailableModel),
}

impl ModelPickerState {
    pub fn new(models: Vec<AvailableModel>, current: Option<usize>) -> Self {
        Self {
            selected: current.unwrap_or(0),
            models,
            current,
            filter: String::new(),
        }
    }

    /// Case-insensitive substring match on the reference and display name.
    pub fn visible(&self) -> Vec<(usize, &AvailableModel)> {
        let query = self.filter.to_ascii_lowercase();
        self.models
            .iter()
            .enumerate()
            .filter(|(_, model)| {
                query.is_empty()
                    || model.reference().to_ascii_lowercase().contains(&query)
                    || model.display_name.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ModelPickerEvent {
        match key.code {
            KeyCode::Esc => ModelPickerEvent::Cancel,
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                ModelPickerEvent::None
            }
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.visible().len().saturating_sub(1));
                ModelPickerEvent::None
            }
            KeyCode::Enter => match self.visible().get(self.selected) {
                Some((_, model)) => ModelPickerEvent::Selected((*model).clone()),
                None => ModelPickerEvent::None,
            },
            KeyCode::Backspace => {
                self.filter.pop();
                self.selected = 0;
                ModelPickerEvent::None
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.filter.push(c);
                self.selected = 0;
                ModelPickerEvent::None
            }
            _ => ModelPickerEvent::None,
        }
    }
}

pub fn render(frame: &mut Frame<'_>, state: &ModelPickerState) {
    let area = popup(frame, 76, 20);
    let block = Block::default()
        .title(" Model ")
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(
            Line::from(" type to filter  ↑↓ choose  Enter select  Esc close ").right_aligned(),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(inner);
    let filter = if state.filter.is_empty() {
        Line::styled(
            "Only models from providers you are logged in to.",
            Style::default().fg(theme::MUTED),
        )
    } else {
        Line::from(vec![
            Span::styled("filter: ", Style::default().fg(theme::MUTED)),
            Span::styled(state.filter.clone(), Style::default().fg(theme::TEXT)),
        ])
    };
    frame.render_widget(Paragraph::new(filter), rows[0]);

    let visible = state.visible();
    if visible.is_empty() {
        let message = if state.models.is_empty() {
            "No logged-in providers. Run /login to connect a provider."
        } else {
            "No models match this filter."
        };
        frame.render_widget(
            Paragraph::new(message).style(Style::default().fg(theme::MUTED)),
            rows[1],
        );
        return;
    }
    let items = visible.iter().enumerate().map(|(row, (index, model))| {
        let selected = row == state.selected;
        let is_current = state.current == Some(*index);
        let label_style = if selected {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let mut spans = vec![
            Span::styled(
                if is_current { "● " } else { "  " },
                Style::default().fg(theme::SUCCESS),
            ),
            Span::styled(format!("{:<28}", model.display_name), label_style),
            Span::styled(model.reference(), Style::default().fg(theme::MUTED)),
        ];
        if is_current {
            spans.push(Span::styled(
                "  current",
                Style::default().fg(theme::SUCCESS),
            ));
        }
        ListItem::new(Line::from(spans)).style(if selected {
            theme::accent_block()
        } else {
            Style::default()
        })
    });
    frame.render_widget(List::new(items).highlight_symbol("›"), rows[1]);
}

#[cfg(test)]
#[path = "model_picker_tests.rs"]
mod tests;

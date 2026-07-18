pub mod approval;
pub mod deck_picker;
pub mod design_picker;
pub mod questionnaire;
pub mod setup;
pub mod template_picker;

pub use approval::render_approval;
pub use deck_picker::DeckPickerState;
pub use design_picker::DesignPickerState;
pub use questionnaire::{Question, QuestionnaireState};
pub use setup::SetupState;
pub use template_picker::TemplatePickerState;

use ratatui::widgets::Clear;
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    Frame,
};

use crate::tui::event::ApprovalRequest;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ModalState {
    #[default]
    None,
    Approval(ApprovalRequest),
    Questionnaire(QuestionnaireState),
    DeckPicker(DeckPickerState),
    TemplatePicker(TemplatePickerState),
    DesignPicker(DesignPickerState),
    Setup(SetupState),
}

pub(crate) fn popup(frame: &mut Frame<'_>, width: u16, height: u16) -> Rect {
    let area = frame.area();
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area)[0];
    let rect = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical)[0];
    frame.render_widget(Clear, rect);
    rect
}

use ratatui::{
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub fn render(frame: &mut Frame<'_>, state: &ModalState) {
    match state {
        ModalState::None => {}
        ModalState::Approval(request) => render_approval(frame, request),
        ModalState::DeckPicker(state) => render_list(
            frame,
            "Open deck",
            state
                .entries
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            state.selected,
        ),
        ModalState::TemplatePicker(state) => render_list(
            frame,
            "Choose template",
            state
                .entries
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            state.selected,
        ),
        ModalState::DesignPicker(state) => render_list(
            frame,
            "Design package",
            state.entries.iter().map(|(name, _)| name.clone()).collect(),
            state.selected,
        ),
        ModalState::Questionnaire(state) => {
            let question = state
                .questions
                .get(state.active)
                .map(|q| q.prompt.as_str())
                .unwrap_or("No questions");
            render_text(
                frame,
                &state.title,
                vec![question.to_owned(), String::new(), state.answer.clone()],
            );
        }
        ModalState::Setup(state) => {
            let diagnostic = state
                .diagnostic
                .clone()
                .unwrap_or_else(|| "Configure decks directory and preview renderer".into());
            render_text(frame, "Setup", vec![diagnostic]);
        }
    }
}

fn render_list(frame: &mut Frame<'_>, title: &str, entries: Vec<String>, selected: usize) {
    let lines = if entries.is_empty() {
        vec![Line::styled(
            "No entries found",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                Line::styled(
                    format!("{} {entry}", if i == selected { "›" } else { " " }),
                    if i == selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect()
    };
    let rect = popup(frame, 72, 18);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).block(
            Block::default()
                .title(format!(" {title} "))
                .borders(Borders::ALL),
        ),
        rect,
    );
}

fn render_text(frame: &mut Frame<'_>, title: &str, body: Vec<String>) {
    let rect = popup(frame, 72, 14);
    frame.render_widget(
        Paragraph::new(body.join("\n"))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .borders(Borders::ALL),
            ),
        rect,
    );
}

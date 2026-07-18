use super::popup;
use crate::tui::event::ApprovalRequest;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render_approval(frame: &mut Frame<'_>, request: &ApprovalRequest) {
    let rect = popup(frame, 72, 12);
    let hint = if request.allow_for_session {
        "[a] allow once   [s] allow for session   [d/esc] deny"
    } else {
        "[a] allow once   [d/esc] deny"
    };
    let text = Text::from(vec![
        Line::styled(
            &request.title,
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from(request.detail.as_str()),
        Line::from(""),
        Line::styled(hint, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Approval required ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        rect,
    );
}

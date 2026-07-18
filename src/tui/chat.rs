use super::app::{App, Focus, Role, ToolStatus, TranscriptItem};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for item in &app.transcript {
        match item {
            TranscriptItem::Message(message) => {
                let (label, color) = match message.role {
                    Role::User => ("You", Color::Cyan),
                    Role::Assistant => ("Agent", Color::Green),
                    Role::System => ("System", Color::Yellow),
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )));
                lines.extend(message.text.lines().map(|line| Line::from(line.to_owned())));
                if !message.complete {
                    lines.push(Line::styled("▌", Style::default().fg(Color::Green)));
                }
                lines.push(Line::from(""));
            }
            TranscriptItem::Tool(card) => {
                let (glyph, color) = match card.status {
                    ToolStatus::Proposed => ("○", Color::DarkGray),
                    ToolStatus::Running => ("◌", Color::Yellow),
                    ToolStatus::Succeeded => ("✓", Color::Green),
                    ToolStatus::Failed => ("✗", Color::Red),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{glyph} {}", card.name),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("  {}", card.summary)),
                ]));
                if !card.detail.is_empty() {
                    lines.push(Line::styled(
                        format!("  {}", card.detail),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                lines.push(Line::from(""));
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::styled(
            "Start by describing the deck you want to build.",
            Style::default().fg(Color::DarkGray),
        ));
    }
    let border = if app.focus == Focus::Chat {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((app.chat_scroll, 0))
            .block(
                Block::default()
                    .title(" Chat ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border)),
            ),
        area,
    );
}

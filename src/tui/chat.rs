use super::{
    app::{App, Focus, Role, ToolStatus, TranscriptItem},
    theme,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
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
                    Role::User => ("You", theme::ACCENT),
                    Role::Assistant => ("Slide-builder", theme::SUCCESS),
                    Role::System => ("Notice", theme::WARNING),
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                )));
                lines.extend(
                    message.text.lines().map(|line| {
                        Line::styled(line.to_owned(), Style::default().fg(theme::TEXT))
                    }),
                );
                if !message.complete {
                    lines.push(Line::styled("▌", Style::default().fg(theme::SUCCESS)));
                }
                lines.push(Line::from(""));
            }
            TranscriptItem::Tool(card) => {
                let (glyph, color) = match card.status {
                    ToolStatus::Proposed => ("○", theme::MUTED),
                    ToolStatus::Running => ("◌", theme::WARNING),
                    ToolStatus::Succeeded => ("✓", theme::SUCCESS),
                    ToolStatus::Failed => ("✗", theme::DANGER),
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{glyph} {}", card.name),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}", card.summary),
                        Style::default().fg(theme::TEXT),
                    ),
                ]));
                if !card.detail.is_empty() {
                    lines.push(Line::styled(
                        format!("  {}", card.detail),
                        Style::default().fg(theme::MUTED),
                    ));
                }
                lines.push(Line::from(""));
            }
        }
    }
    if lines.is_empty() {
        lines.extend([
            Line::from(""),
            Line::styled(
                "Build your deck by describing the outcome.",
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::styled(
                "Try “Create a 6-slide launch narrative” or “Make the active slide more visual.”",
                Style::default().fg(theme::MUTED),
            ),
            Line::from(""),
            Line::styled(
                "Use Ctrl+V before sending to include the active slide.",
                Style::default().fg(theme::MUTED),
            ),
        ]);
    }
    let focused = app.focus == Focus::Chat;
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((app.chat_scroll, 0))
            .block(
                Block::default()
                    .title(Span::styled(
                        " 1  Conversation ",
                        theme::panel_title(focused),
                    ))
                    .title_bottom(
                        Line::styled(" ↑↓ scroll ", Style::default().fg(theme::MUTED))
                            .right_aligned(),
                    )
                    .borders(Borders::ALL)
                    .border_style(theme::panel_border(focused)),
            ),
        area,
    );
}

use super::app::{App, Focus, PreviewStatus};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let count = app.preview.slide_count();
    let title = if count == 0 {
        " Preview ".into()
    } else {
        format!(" Preview · Slide {} / {} ", app.preview.active + 1, count)
    };
    let (message, color) = match &app.preview.status {
        PreviewStatus::Empty => ("No rendered slides", Color::DarkGray),
        PreviewStatus::Rendering { .. } => ("Rendering preview…", Color::Yellow),
        PreviewStatus::Stale { .. } => ("Preview is stale · Ctrl+R to re-render", Color::Yellow),
        PreviewStatus::Failed { error, .. } => (error.as_str(), Color::Red),
        PreviewStatus::Unavailable { reason } => (reason.as_str(), Color::Red),
        PreviewStatus::Ready { .. } => app
            .preview
            .slides
            .get(app.preview.active)
            .and_then(|s| s.image_path.as_ref())
            .and_then(|p| p.to_str())
            .map(|p| (p, Color::DarkGray))
            .unwrap_or(("Rendered slide ready", Color::Green)),
    };
    let border = if app.focus == Focus::Preview {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let text = Text::from(vec![
        Line::from(""),
        Line::styled(message, Style::default().fg(color)),
    ]);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border)),
            ),
        area,
    );
}

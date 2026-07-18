use super::app::App;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let count = app.preview.slide_count();
    let label = if count == 0 {
        "No slide selected".into()
    } else {
        format!("Slide {} / {}", app.preview.active + 1, count)
    };
    let path = app
        .preview
        .slides
        .get(app.preview.active)
        .and_then(|s| s.image_path.as_ref())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "Preview unavailable".into());
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from(path),
            Line::from(""),
            Line::styled(
                "←/→ navigate · f/esc exit",
                Style::default().fg(Color::DarkGray),
            ),
        ]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(format!(" {label} "))
                .borders(Borders::ALL),
        ),
        area,
    );
}

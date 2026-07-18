use super::{app::App, theme};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let count = app.preview.slide_count();
    let label = if count == 0 {
        "No slide selected".into()
    } else {
        format!("Slide {} of {count}", app.preview.active + 1)
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
            Line::styled(
                path,
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::styled(
                "← / → navigate     Esc exit presentation",
                Style::default().fg(theme::MUTED),
            ),
        ]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .title(format!(" {label} "))
                .title_style(
                    Style::default()
                        .fg(theme::TEXT)
                        .add_modifier(Modifier::BOLD),
                )
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::ACCENT)),
        ),
        area,
    );
}

use super::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let tokens = app
        .token_usage
        .map(|(used, limit)| format!("{used}/{limit} tokens"))
        .unwrap_or_else(|| "tokens -".into());
    let run = if app.run_active { "  ● running" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", app.deck_name),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(format!(
                "· {} · {} · {} · {}{}",
                app.design_name, app.mode, app.model, tokens, run
            )),
        ]))
        .style(Style::default().bg(Color::Rgb(25, 25, 30))),
        area,
    );
}

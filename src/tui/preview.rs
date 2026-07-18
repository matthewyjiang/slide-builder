use super::{
    app::{App, PreviewStatus},
    theme,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let count = app.preview.slide_count();
    let title = if count == 0 {
        " Preview ".into()
    } else {
        format!(" Preview  ·  {} / {count} ", app.preview.active + 1)
    };
    let regions = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::styled(title, theme::panel_title())),
        regions[0],
    );

    let (message, detail, color) = match &app.preview.status {
        PreviewStatus::Empty => (
            "No preview yet",
            "Send a prompt to build the deck, then refresh with Ctrl+R.",
            theme::MUTED,
        ),
        PreviewStatus::Rendering { .. } => (
            "Rendering preview…",
            "The workspace remains available while this finishes.",
            theme::WARNING,
        ),
        PreviewStatus::Stale { .. } => (
            "Preview is out of date",
            "Press Ctrl+R to render the latest deck.",
            theme::WARNING,
        ),
        PreviewStatus::Failed { error, .. } => {
            ("Preview could not render", error.as_str(), theme::DANGER)
        }
        PreviewStatus::Unavailable { reason } => {
            ("Preview unavailable", reason.as_str(), theme::DANGER)
        }
        PreviewStatus::Ready { .. } => app
            .preview
            .slides
            .get(app.preview.active)
            .and_then(|s| s.image_path.as_ref())
            .and_then(|p| p.to_str())
            .map(|path| ("Rendered slide", path, theme::MUTED))
            .unwrap_or((
                "Slide ready",
                "Use Present from the actions menu.",
                theme::SUCCESS,
            )),
    };
    let text = Text::from(vec![
        Line::from(""),
        Line::styled(
            message,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(detail, Style::default().fg(theme::MUTED)),
    ]);
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        regions[1],
    );
}

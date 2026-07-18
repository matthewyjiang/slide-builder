use super::{
    app::{App, PreviewStatus},
    preview_image::{ImageRenderStatus, PreviewImage},
    theme,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Paragraph, Wrap},
    Frame,
};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    preview_image: Option<&mut PreviewImage>,
) {
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
    let content_area = regions[1];

    if let PreviewStatus::Ready { .. } = &app.preview.status {
        if let (Some(image), Some(path)) = (
            preview_image,
            app.preview
                .slides
                .get(app.preview.active)
                .and_then(|slide| slide.image_path.as_deref()),
        ) {
            let status = image.render(frame, content_area, path);
            let fullscreen = frame.area();
            image.warm_for_sizes(
                path,
                &[
                    content_area.as_size(),
                    ratatui::layout::Size::new(
                        fullscreen.width.saturating_sub(2),
                        fullscreen.height.saturating_sub(2),
                    ),
                ],
            );
            match status {
                ImageRenderStatus::Ready => {}
                ImageRenderStatus::Loading => render_message(
                    frame,
                    content_area,
                    "Preparing slide…",
                    "Optimizing this slide for your terminal.",
                    theme::ACCENT,
                ),
                ImageRenderStatus::Error(error) => render_message(
                    frame,
                    content_area,
                    "Preview image could not display",
                    &error,
                    theme::DANGER,
                ),
            }
            return;
        }
    }

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
        PreviewStatus::Ready { .. } => (
            "Slide ready",
            "Terminal image output is unavailable.",
            theme::SUCCESS,
        ),
    };
    render_message(frame, content_area, message, detail, color);
}

fn render_message(
    frame: &mut Frame<'_>,
    area: Rect,
    message: &str,
    detail: &str,
    color: ratatui::style::Color,
) {
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
        area,
    );
}

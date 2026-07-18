use super::{
    app::App,
    preview_image::{ImageRenderStatus, PreviewImage},
    theme,
};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn render(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    preview_image: Option<&mut PreviewImage>,
) {
    let count = app.preview.slide_count();
    let label = if count == 0 {
        "No slide selected".into()
    } else {
        format!("Slide {} of {count}", app.preview.active + 1)
    };
    let block = Block::default()
        .title(format!(" {label} "))
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(
            Line::styled(
                " ← / → navigate     Esc exit presentation ",
                Style::default().fg(theme::MUTED),
            )
            .right_aligned(),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let content_area = block.inner(area);
    frame.render_widget(block, area);

    let (message, detail, color) = if let (Some(image), Some(path)) = (
        preview_image,
        app.preview
            .slides
            .get(app.preview.active)
            .and_then(|slide| slide.image_path.as_deref()),
    ) {
        let status = image.render(frame, content_area, path);
        image.warm_for_sizes(path, &[content_area.as_size()]);
        match status {
            ImageRenderStatus::Ready => return,
            ImageRenderStatus::Loading => (
                "Preparing slide…".to_string(),
                "It will appear automatically when ready.".to_string(),
                theme::ACCENT,
            ),
            ImageRenderStatus::Error(error) => (
                "Preview image unavailable".to_string(),
                error,
                theme::DANGER,
            ),
        }
    } else {
        (
            "Preview image unavailable".to_string(),
            "No slide image is available.".to_string(),
            theme::MUTED,
        )
    };

    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::styled(
                message,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::styled(detail, Style::default().fg(theme::MUTED)),
        ]))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true }),
        content_area,
    );
}

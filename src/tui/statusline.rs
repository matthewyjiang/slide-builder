use super::{
    app::{App, Focus, PreviewStatus},
    theme,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let columns =
        Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " SLIDE BUILDER ",
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::ACCENT_SOFT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", app.deck_name),
                Style::default().fg(theme::TEXT),
            ),
            Span::styled(
                format!("  /  {}", app.design_name),
                Style::default().fg(theme::MUTED),
            ),
        ]))
        .style(Style::default().bg(theme::SURFACE)),
        columns[0],
    );

    let count = app.preview.slide_count();
    let slide = if count == 0 {
        "No slides".into()
    } else {
        format!("Slide {} of {count}", app.preview.active + 1)
    };
    let (state, color) = if app.run_active {
        ("● Agent working", theme::WARNING)
    } else {
        match app.preview.status {
            PreviewStatus::Rendering { .. } => ("● Rendering", theme::WARNING),
            PreviewStatus::Stale { .. } => ("● Preview stale", theme::WARNING),
            PreviewStatus::Failed { .. } | PreviewStatus::Unavailable { .. } => {
                ("● Preview issue", theme::DANGER)
            }
            _ => ("● Ready", theme::SUCCESS),
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(state, Style::default().fg(color)),
            Span::styled(format!("  ·  {slide} "), Style::default().fg(theme::MUTED)),
        ]))
        .alignment(Alignment::Right)
        .style(Style::default().bg(theme::SURFACE)),
        columns[1],
    );
}

pub fn render_actions(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let mut spans = vec![
        key("Tab"),
        text(" panel  "),
        key("Ctrl+K/F2"),
        text(" actions  "),
        key("F1"),
        text(" help  "),
    ];
    match app.focus {
        Focus::Chat => {
            spans.extend([key("↑↓"), text(" scroll  ")]);
        }
        Focus::Preview | Focus::Outline => {
            spans.extend([
                key("←→"),
                text(" slide  "),
                key("Enter"),
                text(" present  "),
            ]);
        }
        Focus::Input => {
            spans.extend([
                key("Enter"),
                text(" send  "),
                key("⇧Enter"),
                text(" newline  "),
            ]);
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::SURFACE)),
        area,
    );
}

fn key(label: &'static str) -> Span<'static> {
    Span::styled(format!(" {label} "), theme::keycap())
}

fn text(label: &'static str) -> Span<'static> {
    Span::styled(label, Style::default().fg(theme::MUTED))
}

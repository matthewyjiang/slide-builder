use super::{
    app::{App, Focus},
    theme,
};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app.preview.slides.iter().enumerate().map(|(index, slide)| {
        let active = index == app.preview.active;
        ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {:>2} ", index + 1),
                if active {
                    Style::default().fg(theme::TEXT).bg(theme::ACCENT_SOFT)
                } else {
                    Style::default().fg(theme::MUTED)
                },
            ),
            Span::styled(
                format!(" {}", slide.title),
                if active {
                    Style::default()
                        .fg(theme::TEXT)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::MUTED)
                },
            ),
        ]))
        .style(if active {
            Style::default().bg(theme::ACCENT_SOFT)
        } else {
            Style::default()
        })
    });
    let focused = app.focus == Focus::Outline;
    frame.render_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(Span::styled(" 3  Slides ", theme::panel_title(focused)))
                    .title_bottom(
                        Line::styled(
                            " ←→ select  Enter present ",
                            Style::default().fg(theme::MUTED),
                        )
                        .right_aligned(),
                    )
                    .borders(Borders::ALL)
                    .border_style(theme::panel_border(focused)),
            )
            .highlight_symbol("›"),
        area,
    );
}

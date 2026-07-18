use super::{app::App, theme};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::styled(" Slides ", theme::panel_title())),
        regions[0],
    );

    let items = app.preview.slides.iter().enumerate().map(|(index, slide)| {
        let active = index == app.preview.active;
        let hovered = app.mouse.hovered_slide == Some(index);
        ListItem::new(Line::from(vec![
            Span::styled(
                format!(" {:>2} ", index + 1),
                if active {
                    theme::accent_block()
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
            theme::accent_block()
        } else if hovered {
            theme::hover_block()
        } else {
            Style::default()
        })
    });
    let body_height = regions[1].height as usize;
    let mut state = ListState::default().with_offset(visible_start(app, body_height));
    if !app.preview.slides.is_empty() {
        state.select(Some(app.preview.active));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("›"),
        regions[1],
        &mut state,
    );
}

pub(crate) fn visible_start(app: &App, height: usize) -> usize {
    if height == 0 || app.preview.active < height {
        0
    } else {
        app.preview.active + 1 - height
    }
}

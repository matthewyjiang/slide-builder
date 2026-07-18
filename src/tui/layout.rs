use super::{
    app::{App, Focus},
    chat, modal, outline, preview, slideshow, statusline, theme,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if app.fullscreen {
        slideshow::render(frame, area, app);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(8),
            Constraint::Length(input_height(app, area)),
            Constraint::Length(1),
        ])
        .split(area);
    statusline::render_header(frame, rows[0], app);
    render_workspace(frame, rows[1], app);
    render_input(frame, rows[2], app);
    render_slash_commands(frame, rows[2], app);
    statusline::render_actions(frame, rows[3], app);
    modal::render(frame, &app.modal);
}

fn render_workspace(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let compact = area.width < 88 || area.height < 17;
    if compact {
        match app.focus {
            Focus::Preview => preview::render(frame, area, app),
            Focus::Outline => outline::render(frame, area, app),
            Focus::Chat | Focus::Input => chat::render(frame, area, app),
        }
        return;
    }

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(area);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(body[1]);
    chat::render(frame, body[0], app);
    preview::render(frame, right[0], app);
    outline::render(frame, right[1], app);
}

fn input_height(app: &App, area: Rect) -> u16 {
    let lines = app.input.text.lines().count().max(1) as u16;
    lines
        .saturating_add(2)
        .clamp(3, area.height.saturating_div(3).max(3))
}

fn render_input(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let focused = app.focus == Focus::Input;
    let attachment = if app.input.attach_active_slide {
        Span::styled(
            "  ● active slide attached ",
            Style::default().fg(theme::WARNING),
        )
    } else {
        Span::raw("")
    };
    let title = Line::from(vec![
        Span::styled(" 4  Prompt ", theme::panel_title(focused)),
        attachment,
    ]);
    let slash_suggestions = app.input.slash_suggestions();
    let hint = if app.run_active {
        " Esc cancel current run "
    } else if !slash_suggestions.is_empty() {
        " ↑↓ choose  Tab complete  Enter run "
    } else {
        " Ctrl+V attach slide "
    };
    let display = if app.input.text.is_empty() {
        Text::from(Line::styled(
            "Describe what to create, revise, or explore...",
            Style::default().fg(theme::MUTED),
        ))
    } else {
        Text::from(app.input.text.as_str())
    };
    frame.render_widget(
        Paragraph::new(display).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(title)
                .title_bottom(Line::from(hint).right_aligned())
                .borders(Borders::ALL)
                .border_style(theme::panel_border(focused)),
        ),
        area,
    );

    if focused && !app.run_active && area.width > 2 && area.height > 2 {
        let before = &app.input.text[..app.input.cursor];
        let row = before.chars().filter(|c| *c == '\n').count() as u16;
        let col = before.rsplit('\n').next().unwrap_or("").chars().count() as u16;
        frame.set_cursor_position((
            area.x + 1 + col.min(area.width - 2),
            area.y + 1 + row.min(area.height - 2),
        ));
    }
}

fn render_slash_commands(frame: &mut Frame<'_>, input_area: Rect, app: &App) {
    if app.focus != Focus::Input || app.run_active || !matches!(app.modal, modal::ModalState::None)
    {
        return;
    }
    let suggestions = app.input.slash_suggestions();
    if suggestions.is_empty() || input_area.y < 3 {
        return;
    }

    let visible_rows = suggestions
        .len()
        .min(6)
        .min(input_area.y.saturating_sub(2) as usize);
    if visible_rows == 0 {
        return;
    }
    let selected = app.input.slash_selection.min(suggestions.len() - 1);
    let start = selected
        .saturating_add(1)
        .saturating_sub(visible_rows)
        .min(suggestions.len() - visible_rows);
    let width = input_area.width.saturating_sub(2).clamp(1, 72);
    let height = visible_rows as u16 + 2;
    let area = Rect::new(input_area.x + 1, input_area.y - height, width, height);
    let items = suggestions[start..start + visible_rows]
        .iter()
        .enumerate()
        .map(|(offset, suggestion)| {
            let is_selected = start + offset == selected;
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<12}", suggestion.name),
                    Style::default()
                        .fg(theme::TEXT)
                        .add_modifier(if is_selected {
                            ratatui::style::Modifier::BOLD
                        } else {
                            ratatui::style::Modifier::empty()
                        }),
                ),
                Span::styled(suggestion.detail, Style::default().fg(theme::MUTED)),
            ]))
            .style(if is_selected {
                Style::default().bg(theme::ACCENT_SOFT)
            } else {
                Style::default()
            })
        });

    frame.render_widget(Clear, area);
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title(" Slash commands ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::ACCENT)),
        ),
        area,
    );
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;

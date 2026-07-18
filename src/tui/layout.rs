use super::{
    app::{App, Focus},
    chat, modal, outline, preview, slideshow, statusline,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
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
            Constraint::Min(10),
            Constraint::Length(input_height(app, area)),
            Constraint::Length(1),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(rows[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(body[1]);
    chat::render(frame, body[0], app);
    preview::render(frame, right[0], app);
    outline::render(frame, right[1], app);
    render_input(frame, rows[1], app);
    statusline::render(frame, rows[2], app);
    modal::render(frame, &app.modal);
}

fn input_height(app: &App, area: Rect) -> u16 {
    let lines = app.input.text.lines().count().max(1) as u16;
    lines
        .saturating_add(2)
        .clamp(3, area.height.saturating_div(3).max(3))
}

fn render_input(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let border = if app.focus == Focus::Input {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let attachment = if app.input.attach_active_slide {
        Span::styled("  📎 active slide", Style::default().fg(Color::Yellow))
    } else {
        Span::raw("")
    };
    let title = Line::from(vec![Span::raw(" Message "), attachment]);
    let hint = if app.run_active {
        "Agent is running · Esc to cancel"
    } else {
        "Enter send · Shift+Enter newline · Ctrl+V attach slide"
    };
    frame.render_widget(
        Paragraph::new(app.input.text.as_str())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(title)
                    .title_bottom(Line::from(hint).right_aligned())
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border)),
            ),
        area,
    );

    if app.focus == Focus::Input && !app.run_active && area.width > 2 && area.height > 2 {
        let before = &app.input.text[..app.input.cursor];
        let row = before.chars().filter(|c| *c == '\n').count() as u16;
        let col = before.rsplit('\n').next().unwrap_or("").chars().count() as u16;
        frame.set_cursor_position((
            area.x + 1 + col.min(area.width - 2),
            area.y + 1 + row.min(area.height - 2),
        ));
    }
}

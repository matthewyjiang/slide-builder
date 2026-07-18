use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::theme;

pub fn render(frame: &mut Frame<'_>) {
    let area = centered(frame.area(), 78, 28);
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Keyboard help ")
        .title_style(
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .title_bottom(Line::from(" Esc close ").right_aligned())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let columns = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .spacing(2)
        .split(inner);
    render_section(
        frame,
        columns[0],
        "Workspace",
        &[
            ("Alt+S", "Open the slide prefix"),
            ("Ctrl+K / F2", "Open all actions"),
            ("Ctrl+,", "Open settings"),
            ("F1", "Show this help"),
            ("Enter", "Send prompt"),
            ("Shift+Enter", "New prompt line"),
            ("Ctrl+V", "Attach active slide"),
            ("Ctrl+C", "Clear prompt, or quit when empty"),
        ],
    );
    render_section(
        frame,
        columns[1],
        "Slide prefix (Alt+S, then key)",
        &[
            ("h / k", "Previous slide"),
            ("j / l", "Next slide"),
            ("g / G", "First / last slide"),
            ("r", "Refresh preview"),
            ("Enter / f", "Present active slide"),
            ("Esc / Alt+S", "Cancel prefix"),
            ("Ctrl+R", "Refresh without prefix"),
        ],
    );
}

fn render_section(frame: &mut Frame<'_>, area: Rect, title: &str, shortcuts: &[(&str, &str)]) {
    let mut lines = vec![
        Line::styled(
            title,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
    ];
    for (keys, description) in shortcuts {
        lines.push(Line::from(Span::styled(
            format!(" {keys} "),
            theme::keycap(),
        )));
        lines.push(Line::styled(
            *description,
            Style::default().fg(theme::MUTED),
        ));
        lines.push(Line::from(""));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: true }),
        area,
    );
}

fn centered(area: Rect, max_width: u16, max_height: u16) -> Rect {
    let width = area.width.saturating_sub(2).min(max_width).max(1);
    let height = area.height.saturating_sub(2).min(max_height).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

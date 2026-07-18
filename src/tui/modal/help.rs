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
            ("Tab / Shift+Tab", "Move between panels"),
            ("Ctrl+1", "Conversation"),
            ("Ctrl+2", "Slide preview"),
            ("Ctrl+3", "Slide list"),
            ("Ctrl+4", "Prompt editor"),
            ("Ctrl+K / F2", "Open all actions"),
            ("Ctrl+,", "Open settings"),
            ("F1", "Show this help"),
        ],
    );
    render_section(
        frame,
        columns[1],
        "Slides and prompts",
        &[
            ("← / →", "Previous / next slide"),
            ("Enter", "Present selected slide"),
            ("F", "Present from slide panel"),
            ("Ctrl+R", "Refresh preview"),
            ("Ctrl+V", "Attach active slide"),
            ("Enter", "Send prompt"),
            ("Shift+Enter", "New prompt line"),
            ("Esc", "Cancel an active run"),
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

use super::app::{App, Focus};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let items = app.preview.slides.iter().enumerate().map(|(index, slide)| {
        let style = if index == app.preview.active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        ListItem::new(Line::from(format!("{:>2}. {}", index + 1, slide.title))).style(style)
    });
    let border = if app.focus == Focus::Outline {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    frame.render_widget(
        List::new(items)
            .block(
                Block::default()
                    .title(" Slides ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border)),
            )
            .highlight_symbol("› "),
        area,
    );
}

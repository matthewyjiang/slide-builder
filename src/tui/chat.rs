use super::{
    app::{App, TranscriptItem},
    conversation_entry, theme,
};
use ratatui::buffer::Buffer;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    text::{Line, Text},
    widgets::{Paragraph, Widget, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let regions = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    frame.render_widget(
        Paragraph::new(Line::styled(" Conversation ", theme::panel_title())),
        regions[0],
    );

    let lines = conversation_lines(app, regions[1].width as usize);
    let scroll = scroll_for(&lines, regions[1], app.conversation_scroll_offset);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        regions[1],
    );
}

fn conversation_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for item in &app.transcript {
        match item {
            TranscriptItem::Message(message) => {
                lines.extend(conversation_entry::render_message(message, width));
            }
            TranscriptItem::Tool(card) => {
                lines.extend(conversation_entry::render_tool(card, width));
            }
        }
    }
    if lines.is_empty() {
        lines = conversation_entry::render_empty_state(width);
    }
    lines
}

fn max_scroll(lines: &[Line<'_>], area: Rect) -> u16 {
    lines
        .len()
        .saturating_sub(area.height as usize)
        .try_into()
        .unwrap_or(u16::MAX)
}

fn scroll_for(lines: &[Line<'_>], area: Rect, offset_from_bottom: u16) -> u16 {
    max_scroll(lines, area).saturating_sub(offset_from_bottom)
}

pub(crate) fn scroll_up(app: &mut App, area: Rect, lines: u16) {
    let maximum = max_scroll(&conversation_lines(app, area.width as usize), area);
    app.conversation_scroll_offset = app
        .conversation_scroll_offset
        .saturating_add(lines)
        .min(maximum);
}

pub(crate) fn scroll_down(app: &mut App, lines: u16) {
    app.conversation_scroll_offset = app.conversation_scroll_offset.saturating_sub(lines);
}

pub(crate) fn visible_text_rows(area: Rect, app: &App) -> Vec<String> {
    if area.width == 0 || area.height <= 1 {
        return vec![];
    }
    let body = Rect::new(0, 0, area.width, area.height - 1);
    let lines = conversation_lines(app, body.width as usize);
    let scroll = scroll_for(&lines, body, app.conversation_scroll_offset);
    let mut buffer = Buffer::empty(body);
    Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .render(body, &mut buffer);
    (0..body.height)
        .map(|y| buffer_row_text(&buffer, y, body.width))
        .collect()
}

fn buffer_row_text(buffer: &Buffer, y: u16, width: u16) -> String {
    let mut row = String::new();
    let mut x = 0;
    while x < width {
        let symbol = buffer[(x, y)].symbol();
        row.push_str(symbol);
        let symbol_width = symbol.width().max(1).min(width.saturating_sub(x) as usize);
        x = x.saturating_add(symbol_width as u16);
    }
    row.trim_end().to_owned()
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;

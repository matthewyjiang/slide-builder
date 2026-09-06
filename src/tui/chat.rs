use super::{
    app::{App, TranscriptItem},
    conversation_entry, theme, tool_activity_render,
};
use crossterm::event::KeyCode;
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
        Paragraph::new(Line::styled(
            if app.tool_activity.focus.is_some() {
                [
                    " Tools · ↑↓ move · Enter expand · PgUp/Dn scroll · Esc back ",
                    " Tools · ↑↓ · Enter expand · Esc back ",
                    " Tools · Esc back ",
                ]
                .into_iter()
                .find(|label| label.width() <= usize::from(area.width))
                .unwrap_or(" Tools ")
            } else {
                " Conversation · Ctrl+B t tools "
            },
            theme::panel_title(),
        )),
        regions[0],
    );

    let (lines, focus) = conversation_content(app, regions[1].width as usize);
    let scroll = content_scroll(&lines, focus, regions[1], app);
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        regions[1],
    );
}

fn conversation_lines(app: &App, width: usize) -> Vec<Line<'static>> {
    conversation_content(app, width).0
}

fn conversation_content(app: &App, width: usize) -> (Vec<Line<'static>>, Option<usize>) {
    let mut lines = Vec::new();
    let mut focus = None;
    let mut groups = app.tool_activity.groups(&app.transcript).into_iter();
    let mut index = 0;
    while let Some(item) = app.transcript.get(index) {
        match item {
            TranscriptItem::Message(message) => {
                lines.extend(conversation_entry::render_message(message, width));
                index += 1;
            }
            TranscriptItem::Tool(_) => {
                let group = groups.next().expect("each tool belongs to a group");
                index = group.end;
                let rendered = tool_activity_render::render(app, group, width);
                if let Some(row) = rendered.focused_row {
                    focus = Some(lines.len() + row);
                }
                lines.extend(rendered.lines);
            }
        }
    }
    if lines.is_empty() {
        lines = conversation_entry::render_empty_state(width);
    }
    (lines, focus)
}

fn content_scroll(lines: &[Line<'_>], focus: Option<usize>, area: Rect, app: &App) -> u16 {
    if let Some(row) = focus {
        let page = usize::from(area.height.saturating_sub(1).max(1));
        row.saturating_add(app.tool_activity.detail_scroll.saturating_mul(page))
            .min(usize::from(max_scroll(lines, area))) as u16
    } else {
        scroll_for(lines, area, app.conversation_scroll_offset)
    }
}

pub(crate) fn handle_activity_key(app: &mut App, key: KeyCode) {
    app.tool_activity.handle_key(key, &app.transcript);
    if matches!(key, KeyCode::PageUp | KeyCode::PageDown) && app.mouse.viewport.height > 0 {
        let area = super::layout::regions(app.mouse.viewport, app).chat;
        let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
        let (lines, focus) = conversation_content(app, body.width as usize);
        if let Some(row) = focus {
            let page = usize::from(body.height.saturating_sub(1).max(1));
            let remaining = usize::from(max_scroll(&lines, body)).saturating_sub(row);
            app.tool_activity.detail_scroll = app
                .tool_activity
                .detail_scroll
                .min(remaining.div_ceil(page));
        }
    }
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
    leave_inspection(app, area);
    let maximum = max_scroll(&conversation_lines(app, area.width as usize), area);
    app.conversation_scroll_offset = app
        .conversation_scroll_offset
        .saturating_add(lines)
        .min(maximum);
}

pub(crate) fn scroll_down(app: &mut App, area: Rect, lines: u16) {
    leave_inspection(app, area);
    app.conversation_scroll_offset = app.conversation_scroll_offset.saturating_sub(lines);
}

fn leave_inspection(app: &mut App, area: Rect) {
    if app.tool_activity.focus.is_some() {
        let (lines, focus) = conversation_content(app, area.width as usize);
        let current = content_scroll(&lines, focus, area, app);
        app.conversation_scroll_offset = max_scroll(&lines, area).saturating_sub(current);
        app.tool_activity.focus = None;
    }
}

pub(crate) fn visible_text_rows(area: Rect, app: &App) -> Vec<String> {
    if area.width == 0 || area.height <= 1 {
        return vec![];
    }
    let body = Rect::new(0, 0, area.width, area.height - 1);
    let (lines, focus) = conversation_content(app, body.width as usize);
    let scroll = content_scroll(&lines, focus, body, app);
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

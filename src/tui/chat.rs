use std::rc::Rc;

use super::{
    app::{App, TranscriptItem},
    conversation_images::ImagePlacement,
    markdown::RenderedMarkdown,
    theme, tool_activity_render,
};
use crossterm::event::KeyCode;
use ratatui::buffer::Buffer;
use ratatui::{
    layout::{Constraint, Layout, Rect, Size},
    text::{Line, Text},
    widgets::{Paragraph, Widget},
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

    // Release the old frame before updating streaming messages. Input can only observe
    // the replacement snapshot after this synchronous draw has finished.
    app.painted_conversation.borrow_mut().take();
    let content = conversation_content(app, regions[1].width as usize);
    let scroll = content_scroll(&content, regions[1], app);
    render_text(&content.chunks, regions[1], frame.buffer_mut(), scroll);
    let padding = u16::from(regions[1].width >= 3);
    let image_area = Rect::new(
        regions[1].x + padding,
        regions[1].y,
        regions[1].width.saturating_sub(padding * 2),
        regions[1].height,
    );
    if matches!(app.modal, super::modal::ModalState::None) {
        let first_row = usize::from(scroll);
        let visible_rows = first_row..first_row + usize::from(image_area.height);
        let mut images = app.conversation_images.borrow_mut();
        for (row, source) in &content.image_requests {
            if visible_rows.contains(row) {
                images.request_visible(source, image_size(app, usize::from(area.width)));
            }
        }
        for image in &content.images {
            image.render(frame, image_area, first_row);
        }
    }
    *app.painted_conversation.borrow_mut() = Some(PaintedConversation {
        chunks: content.chunks,
        line_count: content.line_count,
        area: regions[1],
        scroll,
    });
}

/// Retain shared message paint; only visible text rows are copied into the frame widget.
struct ConversationContent {
    chunks: Vec<Rc<RenderedMarkdown>>,
    line_count: usize,
    focus: Option<usize>,
    images: Vec<ImagePlacement>,
    image_requests: Vec<(usize, String)>,
}

/// Geometry and shared message rows from the last painted conversation frame.
/// This retains no image protocols, so evicted images can be freed immediately.
#[derive(Clone, Debug)]
pub struct PaintedConversation {
    chunks: Vec<Rc<RenderedMarkdown>>,
    line_count: usize,
    area: Rect,
    scroll: u16,
}

pub(crate) fn painted_area(app: &App) -> Option<Rect> {
    app.painted_conversation
        .borrow()
        .as_ref()
        .map(|painted| painted.area)
}

pub(crate) fn painted_text_rows(app: &App) -> Vec<String> {
    let painted = app.painted_conversation.borrow();
    let Some(painted) = painted.as_ref() else {
        return vec![];
    };
    let body = Rect::new(0, 0, painted.area.width, painted.area.height);
    let mut buffer = Buffer::empty(body);
    render_text(&painted.chunks, body, &mut buffer, painted.scroll);
    (0..body.height)
        .map(|y| buffer_row_text(&buffer, y, body.width))
        .collect()
}

fn render_text(chunks: &[Rc<RenderedMarkdown>], area: Rect, buffer: &mut Buffer, scroll: u16) {
    let mut skip = usize::from(scroll);
    let mut remaining = usize::from(area.height);
    let mut visible = Vec::with_capacity(remaining);
    for chunk in chunks {
        if skip >= chunk.lines.len() {
            skip -= chunk.lines.len();
            continue;
        }
        let rows = &chunk.lines[skip..];
        let count = rows.len().min(remaining);
        visible.extend_from_slice(&rows[..count]);
        remaining -= count;
        skip = 0;
        if remaining == 0 {
            break;
        }
    }
    Paragraph::new(Text::from(visible)).render(area, buffer);
}

// Composer growth must not change image keys. Only terminal resize changes this constraint.
fn image_size(app: &App, width: usize) -> Size {
    let inner = if width >= 3 { width - 2 } else { width };
    Size::new(
        u16::try_from(inner).unwrap_or(u16::MAX),
        app.mouse.viewport.height.max(1),
    )
}

fn conversation_content(app: &App, width: usize) -> ConversationContent {
    let mut content = ConversationContent {
        chunks: Vec::new(),
        line_count: 0,
        focus: None,
        images: Vec::new(),
        image_requests: Vec::new(),
    };
    let mut image_cache = app.conversation_images.borrow_mut();
    image_cache.poll();
    let available = image_size(app, width);
    let mut cache = app.markdown_cache.borrow_mut();
    cache.prepare(width, app.transcript.len());
    let mut groups = app.tool_activity.groups(&app.transcript).into_iter();
    let mut index = 0;
    while let Some(item) = app.transcript.get(index) {
        let rendered = match item {
            TranscriptItem::Message(message) => {
                let rendered = cache.message_with_images(index, message, &image_cache, available);
                if !rendered.image_sources.is_empty() {
                    for (row, source) in rendered.image_rows.iter().zip(&rendered.image_sources) {
                        if let Some(image) = image_cache.cached_image(&source.path, available) {
                            let start = row + content.line_count;
                            let end = start + usize::from(image.size().height).max(1);
                            content.images.push(ImagePlacement {
                                image,
                                rows: start..end,
                            });
                        }
                    }
                    content.image_requests.extend(
                        rendered
                            .image_rows
                            .iter()
                            .zip(&rendered.image_sources)
                            .map(|(row, source)| (row + content.line_count, source.path.clone())),
                    );
                }
                index += 1;
                rendered
            }
            TranscriptItem::Tool(_) => {
                let group = groups.next().expect("each tool belongs to a group");
                index = group.end;
                let rendered = tool_activity_render::render(app, group, width);
                if let Some(row) = rendered.focused_row {
                    content.focus = Some(content.line_count + row);
                }
                Rc::new(RenderedMarkdown {
                    lines: rendered.lines,
                    code_blocks: vec![],
                    image_sources: vec![],
                    image_rows: vec![],
                })
            }
        };
        content.line_count += rendered.lines.len();
        content.chunks.push(rendered);
    }
    content
}

/// Resolve code-panel copy targets using the same width and scroll as the drawn feed.
pub(crate) fn code_at(app: &App, point: super::mouse::ScreenPoint) -> Option<String> {
    let painted = app.painted_conversation.borrow();
    let painted = painted.as_ref()?;
    let area = painted.area;
    if point.x >= area.right() || point.y >= area.bottom() {
        return None;
    }
    let mut row = usize::from(point.y.checked_sub(area.y)?) + usize::from(painted.scroll);
    let column = usize::from(point.x.checked_sub(area.x)?);
    for chunk in &painted.chunks {
        if row < chunk.lines.len() {
            return chunk
                .code_blocks
                .iter()
                .find(|block| block.top_line == row && block.copy_columns.contains(&column))
                .map(|block| block.text.clone());
        }
        row -= chunk.lines.len();
    }
    None
}

fn content_scroll(content: &ConversationContent, area: Rect, app: &App) -> u16 {
    if let Some(row) = content.focus {
        let page = usize::from(area.height.saturating_sub(1).max(1));
        row.saturating_add(app.tool_activity.detail_scroll.saturating_mul(page))
            .min(usize::from(max_scroll(content.line_count, area))) as u16
    } else {
        max_scroll(content.line_count, area).saturating_sub(app.conversation_scroll_offset)
    }
}

pub(crate) fn handle_activity_key(app: &mut App, key: KeyCode) {
    app.tool_activity.handle_key(key, &app.transcript);
    if matches!(key, KeyCode::PageUp | KeyCode::PageDown) && app.mouse.viewport.height > 0 {
        let area = super::layout::regions(app.mouse.viewport, app).chat;
        let body = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
        let content = conversation_content(app, body.width as usize);
        if let Some(row) = content.focus {
            let page = usize::from(body.height.saturating_sub(1).max(1));
            let remaining = usize::from(max_scroll(content.line_count, body)).saturating_sub(row);
            app.tool_activity.detail_scroll = app
                .tool_activity
                .detail_scroll
                .min(remaining.div_ceil(page));
        }
    }
}

fn max_scroll(line_count: usize, area: Rect) -> u16 {
    line_count
        .saturating_sub(area.height as usize)
        .try_into()
        .unwrap_or(u16::MAX)
}

pub(crate) fn scroll_up(app: &mut App, lines: u16) {
    leave_inspection(app);
    let maximum = app
        .painted_conversation
        .borrow()
        .as_ref()
        .map_or(0, |painted| max_scroll(painted.line_count, painted.area));
    app.conversation_scroll_offset = app
        .conversation_scroll_offset
        .saturating_add(lines)
        .min(maximum);
}

pub(crate) fn scroll_down(app: &mut App, lines: u16) {
    leave_inspection(app);
    app.conversation_scroll_offset = app.conversation_scroll_offset.saturating_sub(lines);
}

fn leave_inspection(app: &mut App) {
    if app.tool_activity.focus.is_some() {
        if let Some(painted) = app.painted_conversation.borrow().as_ref() {
            app.conversation_scroll_offset =
                max_scroll(painted.line_count, painted.area).saturating_sub(painted.scroll);
        }
        app.tool_activity.focus = None;
    }
}

#[cfg(test)]
pub(crate) fn visible_text_rows(area: Rect, app: &App) -> Vec<String> {
    if area.width == 0 || area.height <= 1 {
        return vec![];
    }
    let body = Rect::new(0, 0, area.width, area.height - 1);
    let content = conversation_content(app, body.width as usize);
    let scroll = content_scroll(&content, body, app);
    let mut buffer = Buffer::empty(body);
    render_text(&content.chunks, body, &mut buffer, scroll);
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

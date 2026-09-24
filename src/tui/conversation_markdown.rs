//! Padded assistant markdown with a retained, append-only rendered prefix.
use std::rc::Rc;

use ratatui::text::{Line, Span};

use super::{
    app::{Message, Role},
    conversation_entry,
    conversation_images::ConversationImages,
    conversation_media::MediaLayout,
    markdown::{self, RenderedMarkdown},
    syntax, theme,
};

/// Escape terminal controls and bidi overrides before parsing or capturing copy text.
fn safe_message_text(text: &str) -> String {
    let mut safe = String::with_capacity(text.len());
    for ch in text.chars() {
        if (ch.is_control() && ch != '\n')
            || matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            safe.extend(ch.escape_default());
        } else {
            safe.push(ch);
        }
    }
    safe
}

fn visible_text(text: &str, complete: bool) -> &str {
    if complete {
        text
    } else {
        &text[..markdown::markdown_preview_end(text)]
    }
}

pub(super) fn render(message: &Message, width: usize, inner_width: usize) -> RenderedMarkdown {
    let safe = safe_message_text(&message.text);
    let text = visible_text(&safe, message.complete);
    let mut rendered = markdown::render_markdown(text, inner_width);
    if rendered.lines.is_empty() {
        rendered.lines.push(Line::default());
    }
    if !message.complete {
        rendered.lines.push(cursor());
    }
    pad(&mut rendered, width, inner_width);
    rendered.lines.insert(0, Line::raw(" ".repeat(width)));
    rendered.lines.push(Line::raw(" ".repeat(width)));
    for block in &mut rendered.code_blocks {
        block.top_line += 1;
    }
    for row in &mut rendered.image_rows {
        *row += 1;
    }
    rendered
}

fn cursor() -> Line<'static> {
    Line::styled("▌", theme::assistant_message().fg(theme::SUCCESS))
}

fn pad(rendered: &mut RenderedMarkdown, width: usize, inner_width: usize) {
    let padding = usize::from(width >= 3);
    for line in &mut rendered.lines {
        let trailing = inner_width.saturating_sub(line.width()) + padding;
        if padding > 0 {
            line.spans.insert(0, Span::raw(" "));
        }
        line.spans.push(Span::raw(" ".repeat(trailing)));
    }
    for block in &mut rendered.code_blocks {
        block.copy_columns.start += padding;
        block.copy_columns.end += padding;
    }
}

#[derive(Clone, Debug, Default)]
struct StreamPrefix {
    safe: String,
    bytes: usize,
    rows: usize,
    blocks: usize,
    images: usize,
    #[cfg(test)]
    rendered_bytes: usize,
}

impl StreamPrefix {
    fn update(&mut self, rendered: &mut RenderedMarkdown, message: &Message, width: usize) {
        let inner_width = if width < 3 { width } else { width - 2 };
        rendered.lines.truncate(self.rows);
        rendered.code_blocks.truncate(self.blocks);
        rendered.image_sources.truncate(self.images);
        rendered.image_rows.truncate(self.images);
        let remaining = &self.safe[self.bytes..];
        let visible = visible_text(remaining, message.complete);
        // Keep the last complete block as lookbehind too: a partial next line
        // may yet become a table separator or a pipe-containing table row.
        let complete_end = visible.rfind('\n').map_or(0, |index| index + 1);
        let stable_end = markdown::incremental_markdown_tail_start(&visible[..complete_end]);
        if stable_end > 0 {
            append_fragment(rendered, &visible[..stable_end], width, inner_width);
            self.rows = rendered.lines.len();
            self.blocks = rendered.code_blocks.len();
            self.images = rendered.image_sources.len();
        }
        let tail = &visible[stable_end..];
        if !tail.is_empty() || self.bytes + stable_end == 0 {
            append_fragment(rendered, tail, width, inner_width);
        }
        #[cfg(test)]
        {
            self.rendered_bytes += visible.len();
        }
        self.bytes += stable_end;
        if !message.complete {
            let mut caret = empty_rendered();
            caret.lines.push(cursor());
            pad(&mut caret, width, inner_width);
            rendered.lines.extend(caret.lines);
        }
        rendered.lines.push(Line::raw(" ".repeat(width)));
    }
}

fn empty_rendered() -> RenderedMarkdown {
    RenderedMarkdown {
        lines: Vec::new(),
        code_blocks: Vec::new(),
        image_sources: Vec::new(),
        image_rows: Vec::new(),
    }
}

fn append_fragment(rendered: &mut RenderedMarkdown, text: &str, width: usize, inner_width: usize) {
    let mut fragment = markdown::render_markdown(text, inner_width);
    pad(&mut fragment, width, inner_width);
    let offset = rendered.lines.len();
    for block in &mut fragment.code_blocks {
        block.top_line += offset;
    }
    for row in &mut fragment.image_rows {
        *row += offset;
    }
    rendered.lines.extend(fragment.lines);
    rendered.code_blocks.extend(fragment.code_blocks);
    rendered.image_sources.extend(fragment.image_sources);
    rendered.image_rows.extend(fragment.image_rows);
}

#[derive(Clone, Debug)]
struct CachedMessage {
    message: Message,
    rendered: Rc<RenderedMarkdown>,
    stream: Option<StreamPrefix>,
    media: MediaLayout,
}

/// One entry per transcript slot, invalidated by content, width or syntax readiness.
/// Tool entries occupy empty slots. Replacing or truncating a session cannot reuse stale paint.
#[derive(Clone, Debug, Default)]
pub struct MessageCache {
    width: usize,
    syntax_ready: bool,
    entries: Vec<Option<CachedMessage>>,
}

impl MessageCache {
    pub(super) fn prepare(&mut self, width: usize, transcript_len: usize) {
        let width = width.max(1);
        let ready = syntax::syntax_set_ready();
        if self.width != width || self.syntax_ready != ready {
            self.entries.clear();
            self.width = width;
            self.syntax_ready = ready;
        }
        self.entries.resize_with(transcript_len, || None);
    }

    pub(super) fn message(&mut self, index: usize, message: &Message) -> Rc<RenderedMarkdown> {
        let cached = &mut self.entries[index];
        if let Some(entry) = cached {
            if entry.message == *message {
                return Rc::clone(&entry.rendered);
            }
            if entry.message.role == message.role
                && !entry.message.complete
                && message.text.starts_with(&entry.message.text)
            {
                if let Some(stream) = entry.stream.as_mut() {
                    entry.media = MediaLayout::default();
                    let appended = &message.text[entry.message.text.len()..];
                    stream.safe.push_str(&safe_message_text(appended));
                    stream.update(Rc::make_mut(&mut entry.rendered), message, self.width);
                    entry.message.text.push_str(appended);
                    entry.message.complete = message.complete;
                    return Rc::clone(&entry.rendered);
                }
            }
        }
        let (rendered, stream) = if message.role == Role::Assistant && !message.complete {
            let mut stream = StreamPrefix {
                safe: safe_message_text(&message.text),
                rows: 1,
                ..StreamPrefix::default()
            };
            let mut rendered = empty_rendered();
            rendered.lines.push(Line::raw(" ".repeat(self.width)));
            stream.update(&mut rendered, message, self.width);
            (rendered, Some(stream))
        } else {
            (
                conversation_entry::render_message_content(message, self.width),
                None,
            )
        };
        let rendered = Rc::new(rendered);
        *cached = Some(CachedMessage {
            message: message.clone(),
            rendered: Rc::clone(&rendered),
            stream,
            media: MediaLayout::default(),
        });
        rendered
    }

    pub(super) fn message_with_images(
        &mut self,
        index: usize,
        message: &Message,
        images: &ConversationImages,
        available: ratatui::layout::Size,
    ) -> Rc<RenderedMarkdown> {
        let original = self.message(index, message);
        self.entries[index]
            .as_mut()
            .expect("message cached above")
            .media
            .render(original, images, available, usize::from(self.width >= 3))
    }
}

#[cfg(test)]
#[path = "conversation_markdown_tests.rs"]
mod tests;

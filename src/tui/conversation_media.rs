//! Replace image fallback rows without losing source-panel copy coordinates.
use std::rc::Rc;

use ratatui::{
    layout::Size,
    text::{Line, Span},
};

use super::{
    conversation_entry::plain_rows, conversation_images::ConversationImages,
    markdown::RenderedMarkdown, theme,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum ImageLayout {
    Pending,
    Ready(Size),
    Failed(String),
}

/// One latest media-adjusted layout per message. Image protocols remain owned by the
/// image cache, so retaining text cannot keep evicted image allocations alive.
#[derive(Clone, Debug, Default)]
pub(super) struct MediaLayout {
    cached: Option<(Vec<ImageLayout>, Rc<RenderedMarkdown>)>,
}

impl MediaLayout {
    pub(super) fn render(
        &mut self,
        original: Rc<RenderedMarkdown>,
        cache: &ConversationImages,
        available: Size,
        padding: usize,
    ) -> Rc<RenderedMarkdown> {
        if original.image_sources.is_empty() {
            return original;
        }
        let layout: Vec<_> = original
            .image_sources
            .iter()
            .map(|source| {
                if let Some(image) = cache.cached_image(&source.path, available) {
                    ImageLayout::Ready(image.size())
                } else if let Some(error) = cache.load_error(&source.path, available) {
                    ImageLayout::Failed(error.to_owned())
                } else {
                    ImageLayout::Pending
                }
            })
            .collect();
        if let Some((_, rendered)) = self
            .cached
            .as_ref()
            .filter(|(previous, _)| *previous == layout)
        {
            return Rc::clone(rendered);
        }
        let rendered = if layout
            .iter()
            .any(|image| !matches!(image, ImageLayout::Pending))
        {
            let mut rendered = (*original).clone();
            apply_image_layout(
                &mut rendered,
                &layout,
                usize::from(available.width),
                padding,
            );
            Rc::new(rendered)
        } else {
            original
        };
        self.cached = Some((layout, Rc::clone(&rendered)));
        rendered
    }
}

fn apply_image_layout(
    rendered: &mut RenderedMarkdown,
    layout: &[ImageLayout],
    width: usize,
    padding: usize,
) {
    let original = std::mem::take(&mut rendered.lines);
    let mut row_offsets = Vec::with_capacity(original.len());
    let mut replacements = rendered.image_rows.iter().copied().zip(layout).peekable();
    for (row, line) in original.into_iter().enumerate() {
        row_offsets.push(rendered.lines.len());
        if replacements.peek().is_some_and(|(index, _)| *index == row) {
            let (_, image) = replacements.next().expect("peeked replacement");
            match image {
                ImageLayout::Ready(size) => {
                    rendered
                        .lines
                        .extend((0..usize::from(size.height).max(1)).map(|_| Line::default()));
                }
                ImageLayout::Pending => rendered.lines.push(line),
                ImageLayout::Failed(error) => {
                    rendered.lines.push(line);
                    let lines = plain_rows(
                        &format!("Image unavailable: {error}"),
                        width,
                        theme::assistant_message(),
                    );
                    rendered.lines.extend(lines.into_iter().map(|mut line| {
                        if padding > 0 {
                            line.spans.insert(0, Span::raw(" ".repeat(padding)));
                        }
                        line
                    }));
                }
            }
        } else {
            rendered.lines.push(line);
        }
    }
    for block in &mut rendered.code_blocks {
        block.top_line = row_offsets[block.top_line];
    }
    for row in &mut rendered.image_rows {
        *row = row_offsets[*row];
    }
}

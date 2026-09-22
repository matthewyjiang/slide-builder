//! Replace image fallback rows without losing source-panel copy coordinates.
use ratatui::{
    layout::Size,
    text::{Line, Span},
};

use super::{
    conversation_entry::plain_rows,
    conversation_images::{ConversationImages, ImagePlacement},
    markdown::RenderedMarkdown,
    theme,
};

pub(super) fn place_images(
    rendered: &mut RenderedMarkdown,
    cache: &mut ConversationImages,
    available: Size,
    padding: usize,
) -> Vec<ImagePlacement> {
    let mut replacements = Vec::new();
    for (source, row) in rendered.image_sources.iter().zip(&rendered.image_rows) {
        let image = cache.cached_image(&source.path, available);
        let diagnostic = cache.load_error(&source.path, available).map(str::to_owned);
        replacements.push((*row, image, diagnostic));
    }
    let original = std::mem::take(&mut rendered.lines);
    let mut row_offsets = Vec::with_capacity(original.len());
    let mut placements = Vec::new();
    let mut replacements = replacements.into_iter().peekable();
    for (row, line) in original.into_iter().enumerate() {
        row_offsets.push(rendered.lines.len());
        if replacements
            .peek()
            .is_some_and(|(index, _, _)| *index == row)
        {
            let (_, image, diagnostic) = replacements.next().expect("peeked replacement");
            if let Some(image) = image {
                let start = rendered.lines.len();
                let height = usize::from(image.size().height).max(1);
                rendered.lines.extend((0..height).map(|_| Line::default()));
                placements.push(ImagePlacement {
                    image,
                    rows: start..start + height,
                });
            } else {
                rendered.lines.push(line);
                if let Some(error) = diagnostic {
                    let lines = plain_rows(
                        &format!("Image unavailable: {error}"),
                        usize::from(available.width),
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
    placements
}

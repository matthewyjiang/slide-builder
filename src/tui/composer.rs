use ratatui::text::Line;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A single cell-based layout shared by composer rendering, sizing, and cursor placement.
/// Input wraps at grapheme boundaries, preserving whitespace and explicit blank lines.
pub(super) struct ComposerLayout {
    pub lines: Vec<Line<'static>>,
    pub cursor: (usize, usize),
}

impl ComposerLayout {
    pub fn new(text: &str, cursor: usize, width: u16) -> Self {
        let width = usize::from(width.max(1));
        let mut lines = Vec::new();
        let mut line = String::new();
        let mut col = 0;
        let mut position = (0, 0);
        for (offset, grapheme) in text.grapheme_indices(true) {
            let newline = grapheme == "\n" || grapheme == "\r\n";
            let cells = grapheme.width();
            if !newline && col > 0 && col + cells > width {
                lines.push(Line::from(std::mem::take(&mut line)));
                col = 0;
            }
            if offset <= cursor && cursor < offset + grapheme.len() {
                position = (lines.len(), col);
            }
            if newline {
                lines.push(Line::from(std::mem::take(&mut line)));
                col = 0;
            } else if cells <= width {
                line.push_str(grapheme);
                col += cells;
            }
        }
        if cursor == text.len() {
            // Reserve an insertion cell after a completely filled final row.
            if col == width {
                lines.push(Line::from(std::mem::take(&mut line)));
                col = 0;
            }
            position = (lines.len(), col);
        }
        lines.push(Line::from(line));
        Self {
            lines,
            cursor: position,
        }
    }
}

#[cfg(test)]
#[path = "composer_tests.rs"]
mod tests;

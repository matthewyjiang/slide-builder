// Adapted from Grok Build's terminal Mermaid renderer:
// https://github.com/xai-org/grok-build/blob/b189869b7755d2b482969acf6c92da3ecfeffd36/crates/codegen/xai-grok-markdown/src/mermaid.rs
// Copyright 2023-2026 SpaceXAI. Licensed under Apache-2.0.
use ratatui::style::Style;
use ratatui::text::Line;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::tui) struct GraphStyles {
    pub(in crate::tui) border: Style,
    pub(in crate::tui) node_text: Style,
    pub(in crate::tui) edge: Style,
    pub(in crate::tui) edge_label: Style,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::tui) struct GraphArt {
    pub(in crate::tui) styled_lines: Vec<Line<'static>>,
    pub(in crate::tui) plain_lines: Vec<String>,
}

pub(in crate::tui) const MAX_LABEL: usize = 28;
pub(in crate::tui) const PAD: usize = 1;
pub(in crate::tui) const GAP_X: usize = 3;
pub(in crate::tui) const GAP_Y: usize = 2;
pub(in crate::tui) const WRAP_WIDTH: usize = 24;
pub(in crate::tui) const MAX_LINES: usize = 256;
pub(in crate::tui) const LABEL_BREAK_CHARS: [char; 4] = ['_', '-', '.', '/'];
/// Wrapped edge labels stack at most this many rows above their arrow, so a
/// labeled rank gap grows by a bounded amount. Three rows hold 36 columns of
/// text even at the tightest wrap rung (12), which exceeds the previous
/// single-line 28-column cap; longer labels keep the ellipsis.
pub(in crate::tui) const EDGE_LABEL_MAX_LINES: usize = 3;
pub(in crate::tui) const CONT: char = '\u{0}';
pub(in crate::tui) const MAX_CANVAS_CELLS: usize = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui) enum Oversize {
    Width,
    Cells { width: usize, height: usize },
}

impl std::fmt::Display for Oversize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Width => f.write_str("graph exceeds the requested pane width"),
            Self::Cells { width, height } => write!(
                f,
                "graph canvas cell budget exceeded: limit {MAX_CANVAS_CELLS}, requested {} ({width} × {height})",
                *width as u128 * *height as u128
            ),
        }
    }
}

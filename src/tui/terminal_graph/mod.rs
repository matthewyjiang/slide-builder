//! Generic terminal graph layout and painting.
//!
//! Mermaid adapts its parsed model to this renderer. Layout and painting do not
//! depend on Mermaid parsing or fallback policy.

mod canvas;
mod drawing;
mod flow;
mod ordering;
mod painter;
mod placement;

pub(in crate::tui) use canvas::{Canvas, Cls as CellClass, D, L, R, STY_SOLID, STY_THICK, U};
pub(in crate::tui) use drawing::{draw_box, draw_seq_text, fit_label, wrap_label};
pub(in crate::tui) use flow::{
    art_from_layout, layout_canvas, layout_flow, over_wrap_rungs, NodeExtra, Placed,
};
#[cfg(test)]
pub(in crate::tui) use painter::MAX_LINES;
pub(in crate::tui) use painter::{GraphArt, GraphStyles, Oversize, PAD, WRAP_WIDTH};

/// Shapes supported by the neutral painter, including borderless text for
/// compact markers such as state start/end stubs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui) enum NodeShape {
    Rect,
    Round,
    Diamond,
    /// Label only: no border. Used for compact markers that still need a
    /// layout node so edges can attach.
    Text,
}

/// The direction used by the generic rank layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui) enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

/// Endpoint decorations used by Mermaid and preserved by the shared router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui) enum EdgeHead {
    None,
    Arrow,
    Circle,
    Cross,
    Triangle,
    DiamondFill,
    DiamondOpen,
}

/// Line glyph treatment. Color and terminal style come from the global edge
/// style; this enum only selects solid, dotted, or thick route glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui) enum EdgeLine {
    Solid,
    Dotted,
    Thick,
}

/// Horizontal alignment for lines in a compartmented node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::tui) enum TextAlignment {
    Left,
    Center,
}

/// Explicit content and alignment for one compartment in a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::tui) struct Compartment {
    pub(in crate::tui) lines: Vec<String>,
    pub(in crate::tui) alignment: TextAlignment,
}

/// A graph node, addressed by its index in the graph's node vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::tui) struct Node {
    pub(in crate::tui) label: String,
    pub(in crate::tui) shape: NodeShape,
}

/// An explicit directed edge from `from` to `to`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::tui) struct Edge {
    pub(in crate::tui) from: usize,
    pub(in crate::tui) to: usize,
    pub(in crate::tui) label: Option<String>,
    pub(in crate::tui) head_to: EdgeHead,
    pub(in crate::tui) head_from: EdgeHead,
    pub(in crate::tui) line: EdgeLine,
}

/// A graph with stable node order and explicit directed edges.
/// Adapters must resolve both edge endpoints to indices in `nodes` before
/// constructing this model. Layout minimizes crossings within each rank.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::tui) struct Graph {
    pub(in crate::tui) nodes: Vec<Node>,
    pub(in crate::tui) edges: Vec<Edge>,
    pub(in crate::tui) direction: Direction,
}

#[cfg(test)]
#[path = "terminal_graph_tests.rs"]
mod tests;

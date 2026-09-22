use pretty_assertions::assert_eq;

use super::{flow_graph, Direction, Node, NodeShape};

// The neutral layout indexes nodes directly. The adapter must resolve both
// endpoints before an edge enters that model, even for an incomplete IR.
#[test]
fn omits_edges_with_unresolved_endpoints() {
    for missing_node in ["A", "B"] {
        let mut parsed = mermaid_rs_renderer::parse_mermaid_strict("flowchart TD\nA --> B")
            .expect("valid flowchart");
        parsed.graph.nodes.remove(missing_node);
        let remaining = if missing_node == "A" { "B" } else { "A" };

        assert_eq!(
            flow_graph(&parsed.graph).layout_graph(),
            crate::tui::terminal_graph::Graph {
                nodes: vec![Node {
                    label: remaining.to_owned(),
                    shape: NodeShape::Rect,
                }],
                edges: Vec::new(),
                direction: Direction::TopDown,
            }
        );
    }
}

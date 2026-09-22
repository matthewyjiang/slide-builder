use pretty_assertions::assert_eq;

use super::{
    art_from_layout, layout_canvas, layout_flow, Canvas, Compartment, Direction, Edge, EdgeHead,
    EdgeLine, Graph, GraphArt, GraphStyles, Node, NodeExtra, NodeShape, Oversize, TextAlignment,
    WRAP_WIDTH,
};

fn node(label: &str) -> Node {
    Node {
        label: label.to_owned(),
        shape: NodeShape::Rect,
    }
}

fn directed(from: usize, to: usize) -> Edge {
    Edge {
        from,
        to,
        label: None,
        head_to: EdgeHead::Arrow,
        head_from: EdgeHead::None,
        line: EdgeLine::Solid,
    }
}

fn top_down(nodes: Vec<Node>, edges: Vec<Edge>) -> Graph {
    Graph {
        nodes,
        edges,
        direction: Direction::TopDown,
    }
}

fn render(graph: &Graph) -> GraphArt {
    layout_flow(graph, &GraphStyles::default(), /*max_width*/ None).unwrap()
}

// Covers: one grapheme must consume its measured terminal width even when it
// contains several positive-width Unicode scalars.
// Owner: terminal graph painter.
#[test]
fn paints_zwj_graphemes_without_shifting_node_borders() {
    let graph = top_down(vec![node("👩\u{200d}💻")], Vec::new());
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "┌────┐".to_owned(),
            "│ 👩\u{200d}💻 │".to_owned(),
            "└────┘".to_owned()
        ]
    );
}

// Covers: self-loop endpoint decorations must match the graph edge model.
// Owner: terminal graph painter.
#[test]
fn paints_self_loop_endpoint_decorations() {
    let mut edge = directed(0, 0);
    edge.head_to = EdgeHead::None;
    edge.head_from = EdgeHead::Circle;
    let graph = top_down(vec![node("Node")], vec![edge]);
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "┌──────┐".to_owned(),
            "│ Node │".to_owned(),
            "└────o─┘".to_owned(),
            "     ││".to_owned(),
            "     ╰╯".to_owned(),
        ]
    );
}

// Covers: a compartment's alignment is explicit even when earlier
// compartments are empty.
// Owner: terminal graph painter.
#[test]
fn aligns_each_compartment_by_its_model() {
    let graph = top_down(vec![node("unused")], Vec::new());
    let extras = [NodeExtra::Compartments(vec![
        Compartment {
            lines: Vec::new(),
            alignment: TextAlignment::Left,
        },
        Compartment {
            lines: vec!["T".to_owned()],
            alignment: TextAlignment::Center,
        },
        Compartment {
            lines: vec!["field".to_owned()],
            alignment: TextAlignment::Left,
        },
    ])];
    let canvas = layout_canvas(&graph, &extras, /*max_width*/ None, WRAP_WIDTH).unwrap();
    let art = art_from_layout(&graph, canvas, &GraphStyles::default());
    assert_eq!(
        art.plain_lines,
        vec![
            " ┌───────┐".to_owned(),
            " │   T   │".to_owned(),
            " ├───────┤".to_owned(),
            " │ field │".to_owned(),
            " └───────┘".to_owned(),
        ]
    );
}

// Covers: an empty graph is a valid empty canvas, not a cell-budget failure.
// Owner: terminal graph layout.
#[test]
fn renders_an_empty_graph_as_empty_art() {
    assert_eq!(
        render(&top_down(Vec::new(), Vec::new())),
        GraphArt {
            styled_lines: Vec::new(),
            plain_lines: Vec::new(),
        }
    );
}

// Covers: crossing minimization orders the lower rank by its parents rather
// than by the input node order.
// Owner: terminal graph layout.
#[test]
fn orders_rank_to_avoid_crossed_edges() {
    let art = render(&top_down(
        vec![node("A"), node("B"), node("X"), node("Y")],
        vec![directed(0, 3), directed(1, 2)],
    ));
    let labels = art
        .plain_lines
        .iter()
        .find(|line| line.contains(" Y "))
        .unwrap();
    assert!(labels.find(" Y ").unwrap() < labels.find(" X ").unwrap());
}

// Covers: every canvas caller gets checked allocation, including products
// that overflow usize. Errors retain the requested dimensions for diagnosis.
// Owner: terminal graph canvas allocation.
#[test]
fn rejects_oversize_canvas_before_allocating() {
    for (width, height) in [(super::painter::MAX_CANVAS_CELLS + 1, 1), (usize::MAX, 2)] {
        assert_eq!(
            Canvas::new(width, height).err(),
            Some(Oversize::Cells { width, height })
        );
    }
    assert_eq!(
        Oversize::Cells {
            width: 2_000_001,
            height: 1
        }
        .to_string(),
        "graph canvas cell budget exceeded: limit 2000000, requested 2000001 (2000001 × 1)"
    );
}

// Covers: every fan-in edge of one target shares a single bus row, so a
// complete bipartite dependency layer renders one arrow drop per target
// instead of weaving a target's edges across several rows.
// Owner: terminal graph bus track assignment.
#[test]
fn fan_in_edges_share_one_bus_row_per_target() {
    let graph = top_down(
        vec![node("first"), node("second"), node("left"), node("right")],
        vec![
            directed(0, 2),
            directed(1, 2),
            directed(0, 3),
            directed(1, 3),
        ],
    );
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            " ┌───────┐  ┌────────┐".to_owned(),
            " │ first │  │ second │".to_owned(),
            " └───┬───┘  └────┬───┘".to_owned(),
            "     ├───────────┤".to_owned(),
            "     ▼           ▼".to_owned(),
            " ┌──────┐    ┌───────┐".to_owned(),
            " │ left │    │ right │".to_owned(),
            " └──────┘    └───────┘".to_owned(),
        ]
    );
}

// Covers: a forward edge that skips a rank must reach its target through the
// right lane and the target's own fan-in bus row, instead of running through
// sibling boxes at the target's center row or crossing unrelated edges.
// Owner: terminal graph skip-edge routing.
#[test]
fn rank_skipping_edge_drops_into_the_target_from_above() {
    let graph = top_down(
        vec![node("setup"), node("review"), node("apply"), node("skip")],
        vec![
            directed(0, 1),
            directed(1, 2),
            directed(1, 3),
            directed(0, 3),
        ],
    );
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "         ┌───────┐".to_owned(),
            "         │ setup │".to_owned(),
            "         └───┬───┘".to_owned(),
            "          ┌──┴───────┐".to_owned(),
            "          ▼          │".to_owned(),
            "     ┌────────┐      │".to_owned(),
            "     │ review │      │".to_owned(),
            "     └────┬───┘      │".to_owned(),
            "     ┌────┤          │".to_owned(),
            "     │    └─────┬────┘".to_owned(),
            "     ▼          ▼".to_owned(),
            " ┌───────┐  ┌──────┐".to_owned(),
            " │ apply │  │ skip │".to_owned(),
            " └───────┘  └──────┘".to_owned(),
        ]
    );
}

// Covers: skip edges whose targets share the full source set must join the
// one merged fan-in bus row instead of adding approach rows that cross other
// edges, so shared ink always means joined edges.
// Owner: terminal graph skip-edge routing and bus track assignment.
#[test]
fn skip_edges_join_the_shared_fan_in_bus_row() {
    let graph = top_down(
        vec![
            node("collect"),
            node("boundaries"),
            node("spaghetti"),
            node("structure"),
            node("apply"),
            node("none"),
        ],
        vec![
            directed(0, 1),
            directed(0, 2),
            directed(0, 3),
            directed(1, 4),
            directed(2, 4),
            directed(3, 4),
            directed(0, 4),
            directed(1, 5),
            directed(2, 5),
            directed(3, 5),
            directed(0, 5),
        ],
    );
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "                  ┌─────────┐".to_owned(),
            "                  │ collect │".to_owned(),
            "                  └─────┬───┘".to_owned(),
            "       ┌────────────────┼───────────────┬───────┐".to_owned(),
            "       ▼                ▼               ▼       │".to_owned(),
            "┌────────────┐    ┌───────────┐   ┌───────────┐ │".to_owned(),
            "│ boundaries │    │ spaghetti │   │ structure │ │".to_owned(),
            "└──────┬─────┘    └─────┬─────┘   └─────┬─────┘ │".to_owned(),
            "       └──────────┬─────┴────┬──────────┴───────┘".to_owned(),
            "                  ▼          ▼".to_owned(),
            "              ┌───────┐  ┌──────┐".to_owned(),
            "              │ apply │  │ none │".to_owned(),
            "              └───────┘  └──────┘".to_owned(),
        ]
    );
}

// Covers: skip and back detours take opposite sides in TD and LR so their
// stems cannot share ink or sit in adjacent same-side columns.
// Owner: terminal graph lane hierarchy.
#[test]
fn skip_and_back_edges_use_opposite_side_lanes() {
    let mut graph = top_down(
        vec![node("start"), node("mid"), node("end")],
        vec![
            directed(0, 1),
            directed(1, 2),
            directed(0, 2),
            labeled_edge(2, 1, "no"),
        ],
    );
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "      ┌───────┐".to_owned(),
            "      │ start │".to_owned(),
            "      └───┬───┘".to_owned(),
            "          ├─────┐".to_owned(),
            "          ▼     │".to_owned(),
            "   no  ┌─────┐  │".to_owned(),
            "┌─────▶│ mid │  │".to_owned(),
            "│      └──┬──┘  │".to_owned(),
            "│         ├─────┘".to_owned(),
            "│         ▼".to_owned(),
            "│      ┌─────┐".to_owned(),
            "└──────┤ end │".to_owned(),
            "       └─────┘".to_owned(),
        ]
    );
    graph.direction = Direction::LeftRight;
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "                ┌──────────┐    no".to_owned(),
            "                │          │".to_owned(),
            "                ▼          │".to_owned(),
            "┌───────┐    ┌─────┐    ┌──┴──┐".to_owned(),
            "│ start ├───▶│ mid ├───▶│ end │".to_owned(),
            "└───┬───┘    └─────┘    └─────┘".to_owned(),
            "    │                      ▲".to_owned(),
            "    └──────────────────────┘".to_owned(),
        ]
    );
}

// Covers: a long skip must wrap outside a nested shorter skip instead of
// taking the inner lane and crossing the short hop's stem.
// Owner: terminal graph lane hierarchy.
#[test]
fn nested_skips_use_outer_lane_for_the_longer_span() {
    let mut graph = top_down(
        vec![node("A"), node("B"), node("M"), node("C"), node("D")],
        vec![
            directed(0, 1),
            directed(1, 2),
            directed(2, 3),
            directed(3, 4),
            directed(0, 4),
            directed(1, 3),
        ],
    );
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            " ┌───┐".to_owned(),
            " │ A │".to_owned(),
            " └─┬─┘".to_owned(),
            "   ├────┐".to_owned(),
            "   ▼    │".to_owned(),
            " ┌───┐  │".to_owned(),
            " │ B │  │".to_owned(),
            " └─┬─┘  │".to_owned(),
            "   ├───┐│".to_owned(),
            "   ▼   ││".to_owned(),
            " ┌───┐ ││".to_owned(),
            " │ M │ ││".to_owned(),
            " └─┬─┘ ││".to_owned(),
            "   ├───┘│".to_owned(),
            "   ▼    │".to_owned(),
            " ┌───┐  │".to_owned(),
            " │ C │  │".to_owned(),
            " └─┬─┘  │".to_owned(),
            "   ├────┘".to_owned(),
            "   ▼".to_owned(),
            " ┌───┐".to_owned(),
            " │ D │".to_owned(),
            " └───┘".to_owned(),
        ]
    );
    graph.direction = Direction::LeftRight;
    assert_eq!(
        render(&graph).plain_lines,
        vec![
            "".to_owned(),
            "┌───┐    ┌───┐    ┌───┐    ┌───┐    ┌───┐".to_owned(),
            "│ A ├───▶│ B ├───▶│ M ├───▶│ C ├───▶│ D │".to_owned(),
            "└─┬─┘    └─┬─┘    └───┘    └───┘    └───┘".to_owned(),
            "  │        │                 ▲        ▲".to_owned(),
            "  │        └─────────────────┘        │".to_owned(),
            "  └───────────────────────────────────┘".to_owned(),
        ]
    );
}

const WRAPPED_EDGE_LABEL: &str = "when the renderer reports a width failure";

fn labeled_edge(from: usize, to: usize, label: &str) -> Edge {
    Edge {
        label: Some(label.to_owned()),
        ..directed(from, to)
    }
}

fn assert_wrapped_label_visible(art: &GraphArt) {
    let text = art.plain_lines.join("\n");
    assert!(
        text.contains("when the renderer") && text.contains("width failure"),
        "expected every wrapped word to remain visible:\n{text}"
    );
}

// Covers: LR forward labels wrap in the inter-column gap, including below a
// sibling in the same rank, without occupied cells dropping later rows.
// Owner: terminal graph LR placement.
#[test]
fn wraps_lr_forward_edge_labels_without_dropping_words() {
    let graph = Graph {
        nodes: vec![node("start"), node("top"), node("end")],
        edges: vec![directed(0, 1), labeled_edge(0, 2, WRAPPED_EDGE_LABEL)],
        direction: Direction::LeftRight,
    };
    assert_wrapped_label_visible(&render(&graph));
}

// Covers: TD back-edge labels wrap in the left gutter without clipping
// off the canvas top or stopping on occupied cells.
// Owner: terminal graph TD placement.
#[test]
fn wraps_td_back_edge_labels_without_dropping_words() {
    let graph = top_down(
        vec![node("start"), node("end")],
        vec![directed(0, 1), labeled_edge(1, 0, WRAPPED_EDGE_LABEL)],
    );
    assert_wrapped_label_visible(&render(&graph));
}

// Covers: LR back-route labels wrap above the top lane instead of stacking
// into node boxes that silently truncate the remaining words.
// Owner: terminal graph LR placement.
#[test]
fn wraps_lr_back_edge_labels_without_dropping_words() {
    let graph = Graph {
        nodes: vec![node("start"), node("end")],
        edges: vec![directed(0, 1), labeled_edge(1, 0, WRAPPED_EDGE_LABEL)],
        direction: Direction::LeftRight,
    };
    assert_wrapped_label_visible(&render(&graph));
}

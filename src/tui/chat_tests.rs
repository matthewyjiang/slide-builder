use super::tool_status_text;
use crate::tui::app::{ToolCard, ToolStatus};

fn shape_card(status: ToolStatus) -> ToolCard {
    ToolCard {
        id: "call-1".into(),
        name: "shape_add".into(),
        summary: "rectangle to slide 3".into(),
        detail: String::new(),
        status,
    }
}

#[test]
fn tool_copy_tracks_the_current_status() {
    assert_eq!(
        tool_status_text(&shape_card(ToolStatus::Proposed)),
        "Add rectangle to slide 3"
    );
    assert_eq!(
        tool_status_text(&shape_card(ToolStatus::Running)),
        "Adding rectangle to slide 3"
    );
    assert_eq!(
        tool_status_text(&shape_card(ToolStatus::Succeeded)),
        "Added rectangle to slide 3"
    );
    assert_eq!(
        tool_status_text(&shape_card(ToolStatus::Failed)),
        "Could not add rectangle to slide 3"
    );
}

#[test]
fn successful_file_tools_use_plain_language() {
    let card = ToolCard {
        id: "call-2".into(),
        name: "write_file".into(),
        summary: "chart.svg".into(),
        detail: String::new(),
        status: ToolStatus::Succeeded,
    };
    assert_eq!(tool_status_text(&card), "Wrote chart.svg");
}

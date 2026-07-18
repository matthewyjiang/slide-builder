use super::*;
use crate::tui::{theme, Message, Role, ToolCard, ToolStatus};

fn message(role: Role, text: &str) -> Message {
    Message {
        role,
        text: text.into(),
        complete: true,
    }
}

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
fn user_messages_are_full_width_background_blocks_without_labels() {
    let lines = render_message(&message(Role::User, "hello"), 12);

    assert_eq!(lines.len(), 3);
    assert!(lines.iter().all(|line| line.width() == 12));
    assert!(lines.iter().all(|line| {
        line.spans
            .iter()
            .all(|span| span.style.bg == Some(theme::SURFACE_RAISED))
    }));
    let content = line_content(&lines);
    assert!(content.contains("hello"));
    assert!(!content.contains("You"));
}

#[test]
fn assistant_and_system_messages_use_distinct_surfaces() {
    let assistant = render_message(&message(Role::Assistant, "answer"), 12);
    let system = render_message(&message(Role::System, "heads up"), 12);

    assert!(assistant
        .iter()
        .all(|line| { line.spans.iter().all(|span| span.style.bg.is_none()) }));
    assert!(system.iter().all(|line| {
        line.spans
            .iter()
            .all(|span| span.style.bg == Some(theme::WARNING_SOFT))
    }));
    let content = format!("{}{}", line_content(&assistant), line_content(&system));
    assert!(!content.contains("Slide-builder"));
    assert!(!content.contains("Notice"));
}

#[test]
fn messages_wrap_inside_horizontal_padding() {
    let lines = render_message(&message(Role::User, "one two three"), 8);

    assert_eq!(lines.len(), 5);
    assert!(lines.iter().all(|line| line.width() == 8));
    assert_eq!(wrap_line("one two three", 6), ["one ", "two ", "three"]);
}

#[test]
fn wrapping_preserves_whitespace_and_unicode_graphemes() {
    assert_eq!(wrap_line("abcd efgh", 4), ["abcd", " efg", "h"]);
    assert_eq!(wrap_line("e\u{301}界👩‍💻", 2), ["e\u{301}", "界", "👩‍💻"]);
    assert_eq!(wrap_line("界", 1), ["界"]);
}

#[test]
fn tool_outputs_use_full_width_status_backgrounds() {
    let mut card = shape_card(ToolStatus::Succeeded);
    card.detail = "updated the shape geometry".into();

    let lines = render_tool(&card, 18);

    assert!(lines.iter().all(|line| line.width() == 18));
    assert!(lines.iter().all(|line| {
        line.spans
            .iter()
            .all(|span| span.style.bg == Some(theme::SUCCESS_SOFT))
    }));
    let compact = line_content(&lines)
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert!(compact.contains("Addedrectangletoslide3"));
    assert!(compact.contains("updatedtheshapegeometry"));
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

fn line_content(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .flat_map(|line| &line.spans)
        .map(|span| span.content.as_ref())
        .collect()
}

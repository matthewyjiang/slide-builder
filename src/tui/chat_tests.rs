use super::*;
use crate::tui::{App, Message, Role, ToolCard, ToolStatus, TranscriptItem};

#[test]
fn latest_rows_remain_visible_after_wrapped_tool_output() {
    let app = App {
        transcript: vec![
            TranscriptItem::Tool(ToolCard {
                id: "call-1".into(),
                name: "write_file".into(),
                summary: "a very long generated artifact name that wraps repeatedly".into(),
                detail: "a detailed tool result that also wraps on a narrow panel".into(),
                status: ToolStatus::Succeeded,
            }),
            TranscriptItem::Message(Message {
                role: Role::Assistant,
                text: "latest answer".into(),
                complete: true,
            }),
        ],
        ..App::default()
    };

    let rows = visible_text_rows(Rect::new(0, 0, 12, 6), &app);

    assert!(rows.iter().any(|row| row.contains("latest")));
    assert!(rows.iter().all(|row| row.chars().count() <= 12));
}

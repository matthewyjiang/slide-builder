use super::*;
use rho_sdk::model::{AssistantMessage, ImageContent, ToolCall, ToolResult};
use serde_json::json;

#[test]
fn retains_system_and_interleaved_tool_exchange_with_images() {
    let call = |id: &str| {
        ContentBlock::ToolCall(ToolCall {
            id: id.into(),
            name: "render_deck".into(),
            arguments: json!({}),
        })
    };
    let result = |id: &str| {
        Message::ToolResult(ToolResult {
            id: id.into(),
            ok: true,
            content: "rendered".into(),
        })
    };
    let messages = vec![
        Message::System("deck instructions".into()),
        Message::user_text("old context".repeat(1_000)),
        Message::assistant(AssistantMessage {
            content: vec![call("a")],
            provenance: None,
            reasoning_summary: Some("inspect the slide".into()),
            provider_context: vec![],
        }),
        Message::Assistant(vec![call("b")]),
        result("a"),
        Message::User(vec![ContentBlock::Image(ImageContent {
            data: "image bytes".into(),
            mime_type: "image/png".into(),
        })]),
        result("b"),
    ];
    let partition = partition(&messages, &[], 1).unwrap();
    assert_eq!(partition.leading, messages[..1]);
    assert_eq!(partition.older, messages[1..2]);
    assert_eq!(partition.recent, messages[2..]);
    let rendered = summary_request(&messages);
    let Message::User(blocks) = &rendered[1] else {
        panic!("summary input")
    };
    let ContentBlock::Text(text) = &blocks[0] else {
        panic!("text input")
    };
    assert!(text.contains("inspect the slide"));
    assert!(text.contains("[image: image/png]"));
    assert!(!text.contains("image bytes"));
}

#[test]
fn keeps_a_tail_that_fits_and_never_drops_the_only_exchange() {
    let messages = vec![
        Message::System("system".into()),
        Message::user_text("old".repeat(1_000)),
        Message::user_text("recent"),
    ];
    assert!(partition(&messages, &[], 100_000).is_none());
    assert!(partition(&messages[..2], &[], 1).is_none());
    let partition = partition(&messages, &[], 700).unwrap();
    assert_eq!(partition.recent, messages[2..]);
}

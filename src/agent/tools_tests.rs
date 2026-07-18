use super::{positive_index, UiTool, UiToolCommand};
use rho_sdk::{
    tool::{tool_progress_channel, Tool, ToolContext, ToolInvocation},
    CancellationToken, ToolCallId,
};
use serde_json::json;
use std::{num::NonZeroUsize, path::PathBuf, str::FromStr};
use tokio::sync::mpsc;

#[test]
fn active_slide_index_must_be_positive_and_in_range() {
    assert_eq!(positive_index(&json!({"index": 2})).unwrap(), 2);
    assert!(positive_index(&json!({"index": 0})).is_err());
    assert!(positive_index(&json!({"index": "2"})).is_err());
}

#[tokio::test]
async fn render_tool_waits_for_completed_image_paths() {
    let (commands, mut receiver) = mpsc::unbounded_channel();
    let tool = UiTool::render(commands);
    let invocation = ToolInvocation::new(
        ToolCallId::from_str("render-1").unwrap(),
        serde_json::json!({}),
    );
    let (progress, _) = tool_progress_channel(NonZeroUsize::new(1).unwrap());
    let context = ToolContext::new(None, CancellationToken::new(), progress);

    let respond = async {
        let UiToolCommand::Render { response } = receiver.recv().await.unwrap() else {
            panic!("expected render command")
        };
        response
            .send(Ok(vec![
                PathBuf::from("/tmp/slide-1.png"),
                PathBuf::from("/tmp/slide-2.png"),
            ]))
            .unwrap();
    };
    let (output, ()) = tokio::join!(tool.call(invocation, context), respond);
    let content = output.unwrap().content().to_owned();

    assert!(content.contains("Rendered 2 slides."));
    assert!(content.contains("slide 1: /tmp/slide-1.png"));
    assert!(content.contains("not attached to the model"));
}

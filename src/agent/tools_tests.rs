use super::{positive_index, UiTool, UiToolCommand};
use rho_sdk::{
    tool::{tool_progress_channel, Tool, ToolContext, ToolInvocation},
    CancellationToken, ToolCallId,
};
use serde_json::json;
use std::{num::NonZeroUsize, str::FromStr};
use tokio::sync::mpsc;

#[test]
fn active_slide_index_must_be_positive_and_in_range() {
    assert_eq!(positive_index(&json!({"index": 2})).unwrap(), 2);
    assert!(positive_index(&json!({"index": 0})).is_err());
    assert!(positive_index(&json!({"index": "2"})).is_err());
}

#[tokio::test]
async fn render_tool_waits_for_completed_image_paths() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (1..=2)
        .map(|index| directory.path().join(format!("slide-{index}.png")))
        .collect();
    for path in &paths {
        image::RgbImage::from_pixel(2, 2, image::Rgb([0, 48, 87]))
            .save(path)
            .unwrap();
    }
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
        response.send(Ok(paths.clone())).unwrap();
    };
    let (output, ()) = tokio::join!(tool.call(invocation, context), respond);
    let output = output.unwrap();
    let content = output.content();

    assert!(content.contains("Rendered 2 slides."));
    assert!(content.contains(&format!("slide 1: {}", paths[0].display())));
    assert_eq!(output.presentation().assets().len(), paths.len());
    for (asset, path) in output.presentation().assets().iter().zip(paths) {
        assert_eq!(asset.media_type(), "image/png");
        assert_eq!(asset.bytes(), std::fs::read(path).unwrap());
    }
}

#[tokio::test]
async fn missing_rendered_image_fails_with_slide_and_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.png");
    let (commands, mut receiver) = mpsc::unbounded_channel();
    let tool = UiTool::render(commands);
    let invocation = ToolInvocation::new(ToolCallId::from_str("render-1").unwrap(), json!({}));
    let (progress, _) = tool_progress_channel(NonZeroUsize::new(1).unwrap());
    let context = ToolContext::new(None, CancellationToken::new(), progress);
    let respond = async {
        let UiToolCommand::Render { response } = receiver.recv().await.unwrap() else {
            panic!("expected render")
        };
        response.send(Ok(vec![path.clone()])).unwrap();
    };
    let (output, ()) = tokio::join!(tool.call(invocation, context), respond);
    let error = output.unwrap_err();
    assert_eq!(error.kind(), rho_sdk::tool::ToolErrorKind::Execution);
    assert!(error
        .message()
        .contains("could not attach rendered slide 1"));
    assert!(error.message().contains(&path.display().to_string()));
}

#[tokio::test]
async fn cancelling_pending_render_drops_the_image_response() {
    let (commands, mut receiver) = mpsc::unbounded_channel();
    let tool = UiTool::render(commands);
    let invocation = ToolInvocation::new(ToolCallId::from_str("render-1").unwrap(), json!({}));
    let (progress, _) = tool_progress_channel(NonZeroUsize::new(1).unwrap());
    let cancellation = CancellationToken::new();
    let context = ToolContext::new(None, cancellation.clone(), progress);
    let cancel = async {
        let UiToolCommand::Render { mut response } = receiver.recv().await.unwrap() else {
            panic!("expected render")
        };
        cancellation.cancel();
        response.closed().await;
        assert!(response.send(Ok(vec!["stale-slide.png".into()])).is_err());
    };
    let (output, ()) = tokio::join!(tool.call(invocation, context), cancel);
    assert_eq!(
        output.unwrap_err().kind(),
        rho_sdk::tool::ToolErrorKind::Cancelled
    );
}

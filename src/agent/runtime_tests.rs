use super::adapt_run_event;
use crate::tui::{AgentEvent, AppEvent};
use rho_sdk::{model::ToolCall, tool::ToolOutput, RunEvent, ToolCallId, ToolCompletion};
use serde_json::json;

#[tokio::test]
async fn rendered_slides_reach_the_next_model_request() {
    use super::AgentHandle;
    use crate::agent::tools::{UiTool, UiToolCommand};
    use rho_sdk::{
        model::{ContentBlock, ImageContent, Message, ModelIdentity, ModelResponse},
        provider::{ScriptedProvider, ScriptedTurn},
        Rho,
    };
    use tokio::sync::mpsc;

    let directory = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (1..=2)
        .map(|index| directory.path().join(format!("slide-{index}.png")))
        .collect();
    for (index, path) in paths.iter().enumerate() {
        image::RgbImage::from_pixel(2, 2, image::Rgb([index as u8, 48, 87]))
            .save(path)
            .unwrap();
    }
    let expected: Vec<_> = paths
        .iter()
        .map(|path| ImageContent {
            data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                std::fs::read(path).unwrap(),
            ),
            mime_type: "image/png".into(),
        })
        .collect();
    let provider = ScriptedProvider::new(
        ModelIdentity::new("scripted", "test", "model"),
        [
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::ToolCall(
                ToolCall {
                    id: "render-1".into(),
                    name: "render_deck".into(),
                    arguments: json!({}),
                },
            )])),
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::Text(
                "inspected".into(),
            )])),
        ],
    );
    let (commands, mut receiver) = mpsc::unbounded_channel();
    let rho = Rho::builder()
        .provider(provider.clone())
        .tool(UiTool::render(commands))
        .event_capacity(std::num::NonZeroUsize::new(1).unwrap())
        .build()
        .unwrap();
    let agent = AgentHandle::new(rho).await.unwrap();
    let (events, _) = mpsc::unbounded_channel();
    let respond = async {
        let UiToolCommand::Render { response } = receiver.recv().await.unwrap() else {
            panic!("expected render")
        };
        response.send(Ok(paths)).unwrap();
    };
    let (result, ()) = tokio::join!(
        agent.send("render and inspect".into(), None, events),
        respond
    );
    result.unwrap();
    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 2);
    let images: Vec<_> = requests[1]
        .messages
        .iter()
        .filter_map(|message| match message {
            Message::User(blocks) => Some(blocks),
            _ => None,
        })
        .flatten()
        .filter_map(|block| match block {
            ContentBlock::Image(image) => Some(image.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(images, expected);
}

#[test]
fn proposed_tool_retains_full_arguments_and_existing_summary() {
    let arguments = json!({
        "path": "/tmp/decks/notes.txt",
        "content": "First line\nSecond line: λ",
        "options": { "overwrite": true }
    });
    assert_eq!(
        adapt_run_event(RunEvent::ToolProposed {
            call: ToolCall {
                id: "call-1".into(),
                name: "write_file".into(),
                arguments,
            },
        }),
        vec![AppEvent::Run(AgentEvent::ToolProposed {
            id: "call-1".into(),
            name: "write_file".into(),
            summary: "notes.txt".into(),
            arguments: concat!(
                "{\n",
                "  \"path\": \"/tmp/decks/notes.txt\",\n",
                "  \"content\": \"First line\\nSecond line: λ\",\n",
                "  \"options\": {\n",
                "    \"overwrite\": true\n",
                "  }\n",
                "}"
            )
            .into(),
        })]
    );
}

#[test]
fn finished_tool_retains_successful_output_verbatim() {
    for output in ["", "  first line\nsecond line: λ\n\n"] {
        assert_eq!(
            adapt_run_event(RunEvent::ToolFinished {
                call_id: ToolCallId::from_string("call-1").unwrap(),
                result: ToolCompletion::Success(ToolOutput::text(output)),
            }),
            vec![AppEvent::Run(AgentEvent::ToolFinished {
                id: "call-1".into(),
                result: Ok(output.into()),
            })]
        );
    }
}

#[test]
fn unavailable_tool_preserves_failure_message() {
    assert_eq!(
        adapt_run_event(RunEvent::ToolFinished {
            call_id: ToolCallId::from_string("call-1").unwrap(),
            result: ToolCompletion::Unavailable,
        }),
        vec![AppEvent::Run(AgentEvent::ToolFinished {
            id: "call-1".into(),
            result: Err("tool unavailable".into()),
        })]
    );
}

use super::adapt_run_event;
use crate::tui::{AgentEvent, AppEvent};
use rho_sdk::{model::ToolCall, tool::ToolOutput, RunEvent, ToolCallId, ToolCompletion};
use serde_json::json;

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

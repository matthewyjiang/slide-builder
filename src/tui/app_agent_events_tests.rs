use super::*;
use crate::{
    agent::runtime::adapt_run_event,
    tui::{chat, AppEvent},
};
use pretty_assertions::assert_eq;
use ratatui::{backend::TestBackend, Terminal};
use rho_sdk::{model::ToolCall, tool::ToolOutput, RunEvent, ToolCallId, ToolCompletion};

fn screen(terminal: &mut Terminal<TestBackend>, app: &App) -> String {
    terminal
        .draw(|frame| chat::render(frame, frame.area(), app))
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn streaming_cursor_clears_when_text_hands_off_to_tools() {
    for width in [32, 110] {
        let mut app = App {
            run_active: true,
            ..App::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(width, 32)).unwrap();
        for (id, text) in [
            ("read-1", "Checking notes."),
            ("read-2", "Checking results."),
        ] {
            for event in adapt_run_event(RunEvent::AssistantTextDelta { text: text.into() }) {
                app.apply(event);
            }
            let rendered = screen(&mut terminal, &app);
            assert!(rendered.contains(text));
            assert_eq!(rendered.matches('▌').count(), 1);
            let message_index = app.transcript.len() - 1;

            // The SDK proposes tools before it reports completion of the whole run.
            for event in adapt_run_event(RunEvent::ToolProposed {
                call: ToolCall {
                    id: id.into(),
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "notes.txt"}),
                },
            }) {
                app.apply(event);
            }
            let rendered = screen(&mut terminal, &app);
            assert!(rendered.contains(text));
            assert_eq!(rendered.matches('▌').count(), 0);
            assert_eq!(
                app.transcript[message_index],
                TranscriptItem::Message(Message {
                    role: Role::Assistant,
                    text: text.into(),
                    complete: true,
                })
            );
            for event in adapt_run_event(RunEvent::ToolFinished {
                call_id: ToolCallId::from_string(id).unwrap(),
                result: ToolCompletion::Success(ToolOutput::text("Notes read.")),
            }) {
                app.apply(event);
            }
        }
        app.apply(AppEvent::Run(AgentEvent::TextDelta("Done.".into())));
        assert_eq!(screen(&mut terminal, &app).matches('▌').count(), 1);
        app.apply(AppEvent::Run(AgentEvent::MessageFinished));
        assert_eq!(screen(&mut terminal, &app).matches('▌').count(), 0);
    }
}

#[test]
fn ending_a_run_finalizes_streamed_text_without_message_finished() {
    for ending in [
        AgentEvent::RunFinished,
        AgentEvent::RunCancelled,
        AgentEvent::RunFailed("Connection lost".into()),
    ] {
        let mut app = App {
            run_active: true,
            ..App::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(70, 16)).unwrap();
        app.apply(AppEvent::Run(AgentEvent::TextDelta(
            "Partial response".into(),
        )));
        assert!(screen(&mut terminal, &app).contains('▌'));
        app.apply(AppEvent::Run(ending));
        let rendered = screen(&mut terminal, &app);
        assert!(rendered.contains("Partial response"));
        assert!(!rendered.contains('▌'));
        assert!(!app.run_active);
        assert_eq!(
            app.transcript[0],
            TranscriptItem::Message(Message {
                role: Role::Assistant,
                text: "Partial response".into(),
                complete: true,
            })
        );
    }
}

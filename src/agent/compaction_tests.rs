use super::*;
use rho_sdk::{
    model::{Message, ModelIdentity},
    provider::{NativeCompactionFuture, ProviderFuture, ScriptedProvider, ScriptedTurn},
    CancellationToken, CompactionTrigger, Rho, SessionOptions, SystemPrompt,
};

fn history() -> Vec<Message> {
    vec![
        Message::System("deck instructions".into()),
        Message::user_text("old context ".repeat(1_000)),
        Message::assistant_text("old answer ".repeat(1_000)),
        Message::user_text("latest edit"),
    ]
}

fn scripted(texts: &[&str]) -> ScriptedProvider {
    ScriptedProvider::new(
        ModelIdentity::new("scripted", "test", "model"),
        texts.iter().map(|text| {
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::Text(
                (*text).into(),
            )]))
        }),
    )
}

fn compactor(provider: Arc<dyn ModelProvider>) -> ModelCompactor {
    ModelCompactor {
        provider,
        tools: vec![],
        window: Some(2_000),
    }
}

fn request() -> CompactionRequest {
    CompactionRequest::new(history(), CancellationToken::new())
        .with_trigger(CompactionTrigger::Automatic)
}

struct NativeProvider {
    text: ScriptedProvider,
}
impl ModelProvider for NativeProvider {
    fn identity(&self) -> ModelIdentity {
        self.text.identity()
    }
    fn send_turn<'a>(&'a self, request: ModelRequest<'a>) -> ProviderFuture<'a> {
        self.text.send_turn(request)
    }
    fn native_compact<'a>(&'a self, _: ModelRequest<'a>) -> Option<NativeCompactionFuture<'a>> {
        Some(Box::pin(async move {
            panic!("provider-bound native compaction must not be requested")
        }))
    }
}

#[tokio::test]
async fn native_capable_provider_uses_portable_text_summary() {
    let provider = Arc::new(NativeProvider {
        text: scripted(&["portable summary"]),
    });
    let output = compactor(provider.clone())
        .compact(request())
        .await
        .unwrap();
    assert_eq!(
        output.messages(),
        &[
            history()[0].clone(),
            Message::user_text(
                "Summary of earlier conversation for model context only:\n\nportable summary"
            ),
            history()[3].clone(),
        ]
    );
    let requests = provider.text.recorded_requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].tools.is_empty());
    assert_eq!(requests[0].messages, summary_request(&history()[1..3]));
}

#[tokio::test]
async fn empty_or_expanding_summary_fails_without_replacement() {
    for text in ["".to_owned(), "expanded ".repeat(10_000)] {
        let error = compactor(Arc::new(scripted(&[&text])))
            .compact(request())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("History is unchanged"));
    }
}

#[tokio::test]
async fn automatic_compaction_flows_through_agent_ui_and_saved_snapshot() {
    use crate::agent::{
        runtime::AgentHandle,
        session_store::{SessionState, SessionStore},
    };
    use crate::tui::{AgentEvent, App, AppEvent, Message as UiMessage, Role, TranscriptItem};
    let provider = Arc::new(NativeProvider {
        text: scripted(&["remember deck constraints", "continued"]),
    });
    let rho = Rho::builder()
        .provider_shared(provider.clone())
        .system_prompt(SystemPrompt::Custom("deck instructions".into()))
        .compactor(compactor(provider.clone()))
        .compaction_policy(policy(Some(2_000)).unwrap())
        .build()
        .unwrap();
    let session = rho
        .session(SessionOptions::default().history(history()))
        .await
        .unwrap();
    let agent = AgentHandle::restore(rho, session.snapshot(), "deck instructions".into())
        .await
        .unwrap();
    let (events, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    agent
        .send("continue editing".into(), None, events)
        .await
        .unwrap();
    let mut app = App {
        run_active: true,
        ..App::default()
    };
    app.transcript.push(TranscriptItem::Message(UiMessage {
        role: Role::User,
        text: "original transcript".into(),
        complete: true,
    }));
    let mut lifecycle = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let AppEvent::Run(
            AgentEvent::CompactionStarted | AgentEvent::CompactionCompleted { .. },
        ) = &event
        {
            lifecycle.push(event.clone());
        }
        app.apply(event);
    }
    assert!(matches!(
        lifecycle.as_slice(),
        [
            AppEvent::Run(AgentEvent::CompactionStarted),
            AppEvent::Run(AgentEvent::CompactionCompleted { .. })
        ]
    ));
    assert!(!app.run_active);
    assert!(
        matches!(&app.transcript[0], TranscriptItem::Message(message) if message.text == "original transcript")
    );
    assert!(app.transcript.iter().any(|item| matches!(item, TranscriptItem::Message(message) if message.text.contains("Context compacted"))));
    let snapshot = agent.snapshot();
    assert_eq!(snapshot.compaction().completed_compactions(), 1);
    assert!(snapshot.compaction().removed_tokens() > 0);
    let requests = provider.text.recorded_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].messages[0], history()[0]);
    assert!(!requests[1].messages.contains(&history()[1]));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sessions.sqlite3");
    let store = SessionStore::open(&path).unwrap();
    let mut saved = store
        .create(
            session.snapshot(),
            SessionState {
                deck: "deck.pptx".into(),
                cwd: directory.path().into(),
                provider: "scripted".into(),
                auth: "api-key".into(),
                model: "model".into(),
                active_slide: 0,
                design_name: "Studio".into(),
                pending_design_context: None,
                transcript: app.transcript.clone(),
                draft: String::new(),
                attach_active_slide: false,
            },
        )
        .unwrap();
    saved.snapshot = snapshot.clone();
    store.save(&mut saved).unwrap();
    drop(store);
    let loaded = SessionStore::open(&path).unwrap().load(&saved.id).unwrap();
    assert_eq!(loaded.snapshot, snapshot);
    assert_eq!(loaded.state.transcript, app.transcript);
    let resumed_provider = ScriptedProvider::new(
        ModelIdentity::new("another-provider", "test", "another-model"),
        [ScriptedTurn::completed(ModelResponse::Assistant(vec![
            ContentBlock::Text("resumed".into()),
        ]))],
    );
    let resumed = AgentHandle::restore(
        Rho::builder()
            .provider(resumed_provider.clone())
            .build()
            .unwrap(),
        loaded.snapshot,
        "current deck instructions".into(),
    )
    .await
    .unwrap();
    assert_eq!(resumed.snapshot().compaction().completed_compactions(), 1);
    let (events, _) = tokio::sync::mpsc::unbounded_channel();
    resumed.send("next".into(), None, events).await.unwrap();
    assert_eq!(
        resumed_provider.recorded_requests()[0].messages[0],
        Message::System("current deck instructions".into())
    );
    assert!(!resumed_provider.recorded_requests()[0]
        .messages
        .contains(&history()[1]));
    assert!(resumed_provider.recorded_requests()[0]
        .messages
        .contains(&Message::user_text(
            "Summary of earlier conversation for model context only:\n\nremember deck constraints"
        )));
}

#[tokio::test]
async fn sdk_cancels_an_in_flight_summary_without_committing_history() {
    struct Waiting(tokio::sync::Notify);
    impl ModelProvider for Waiting {
        fn identity(&self) -> ModelIdentity {
            ModelIdentity::new("test", "test", "waiting")
        }
        fn send_turn<'a>(&'a self, _: ModelRequest<'a>) -> ProviderFuture<'a> {
            Box::pin(async move {
                self.0.notify_one();
                std::future::pending().await
            })
        }
    }
    let provider = Arc::new(Waiting(tokio::sync::Notify::new()));
    let rho = Rho::builder()
        .provider_shared(provider.clone())
        .compactor(compactor(provider.clone()))
        .compaction_policy(policy(Some(2_000)).unwrap())
        .build()
        .unwrap();
    let session = rho
        .session(SessionOptions::default().history(history()))
        .await
        .unwrap();
    let mut run = session
        .start(rho_sdk::UserInput::text("continue"))
        .await
        .unwrap();
    provider.0.notified().await;
    run.cancellation_handle().cancel();
    while run.next_event().await.is_some() {}
    assert!(matches!(run.outcome().await, Err(Error::Cancelled)));
    assert_eq!(session.snapshot().compaction().completed_compactions(), 0);
    assert!(session.history().starts_with(&history()));
}

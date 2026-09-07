use super::*;
use rho_sdk::{
    model::{ContentBlock, Message, ModelIdentity, ModelResponse, ToolCall},
    provider::{ScriptedProvider, ScriptedTurn},
    Rho, SystemPrompt,
};
use slide_builder::{
    agent::tools::{UiTool, UiToolCommand},
    tui::{AgentEvent, AppEvent, TranscriptItem},
};
use tokio::sync::mpsc;

#[tokio::test]
async fn session_picker_preserves_database_order_and_marks_current_session() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::open(&directory.path().join("sessions.sqlite3")).unwrap();
    let config = Config::default();
    let mut records = Vec::new();
    for deck in ["first.pptx", "second.pptx"] {
        let agent = AgentHandle::new(Rho::builder().provider(provider("unused")).build().unwrap())
            .await
            .unwrap();
        records.push(
            store
                .create(
                    agent.snapshot(),
                    initial_state(&directory.path().join(deck), directory.path(), &config),
                )
                .unwrap(),
        );
    }
    let entries = picker_entries(&store, &records[0].id).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| (&entry.id, entry.current))
            .collect::<Vec<_>>(),
        vec![(&records[1].id, false), (&records[0].id, true)]
    );
    store
        .rename(&records[0].id, "Renamed presentation")
        .unwrap();
    let entries = picker_entries(&store, &records[1].id).unwrap();
    assert_eq!(entries[0].id, records[0].id);
    assert_eq!(entries[0].name, "Renamed presentation");
    assert_eq!(
        entries[0].deck,
        directory.path().join("first.pptx").to_string_lossy()
    );
}

#[tokio::test]
async fn picker_resume_validates_destination_without_mutating_saved_state_or_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deck.pptx");
    DeckEngine::create(&path, None).await.unwrap();
    let original_deck = std::fs::read(&path).unwrap();
    let store = SessionStore::open(&directory.path().join("sessions.sqlite3")).unwrap();
    let agent = AgentHandle::new(
        Rho::builder()
            .provider(provider("saved response"))
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    let (tx, _) = mpsc::unbounded_channel();
    agent.send("saved question".into(), None, tx).await.unwrap();
    let mut record = store
        .create(
            agent.snapshot(),
            initial_state(&path, directory.path(), &Config::default()),
        )
        .unwrap();
    let (restored, engine) = prepare_resume(&store, &record.id).await.unwrap();
    assert_eq!(restored.snapshot, record.snapshot);
    assert_eq!(engine.path(), path);
    assert_eq!(std::fs::read(&path).unwrap(), original_deck);
    assert_eq!(store.load(&record.id).unwrap().revision, record.revision);

    record.state.cwd = directory.path().join("missing-workspace");
    store.save(&mut record).unwrap();
    assert!(prepare_resume(&store, &record.id)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("workspace is missing"));
    record.state.cwd = directory.path().into();
    store.save(&mut record).unwrap();
    std::fs::rename(&path, directory.path().join("moved.pptx")).unwrap();
    assert!(prepare_resume(&store, &record.id)
        .await
        .err()
        .unwrap()
        .to_string()
        .contains("saved deck is missing"));
    assert!(!path.exists());
    std::fs::write(&path, b"not a PowerPoint file").unwrap();
    assert!(prepare_resume(&store, &record.id).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"not a PowerPoint file");
    assert_eq!(store.load(&record.id).unwrap().snapshot, record.snapshot);
}

fn provider(response: &str) -> ScriptedProvider {
    ScriptedProvider::new(
        ModelIdentity::new("scripted", "scripted", "test"),
        [ScriptedTurn::completed(ModelResponse::Assistant(vec![
            ContentBlock::Text(response.into()),
        ]))],
    )
}

#[test]
fn restored_transcript_has_no_live_operations() {
    use slide_builder::tui::{Message as UiMessage, Role, ToolCard, ToolStatus};
    let mut state = initial_state(
        Path::new("/deck.pptx"),
        Path::new("/workspace"),
        &Config::default(),
    );
    state.transcript = vec![
        TranscriptItem::Message(UiMessage {
            role: Role::Assistant,
            text: "partial response".into(),
            complete: false,
        }),
        TranscriptItem::Tool(ToolCard {
            id: "call".into(),
            name: "render_deck".into(),
            summary: String::new(),
            arguments: "{}".into(),
            detail: String::new(),
            status: ToolStatus::Running,
        }),
    ];
    let mut app = App::default();
    restore_app(&mut app, &state, 0);
    assert!(matches!(&app.transcript[0], TranscriptItem::Message(message) if message.complete));
    assert!(
        matches!(&app.transcript[1], TranscriptItem::Tool(card) if card.status == ToolStatus::Failed && card.detail.contains("Session ended"))
    );
    assert!(!app.run_active);
}

#[test]
fn unrelated_configuration_save_keeps_global_model_but_explicit_switch_updates_it() {
    let global = Config {
        provider: "global-provider".into(),
        auth: "global-auth".into(),
        model: "global-model".into(),
        ..Config::default()
    };
    let resumed = Config {
        provider: "session-provider".into(),
        auth: "session-auth".into(),
        model: "session-model".into(),
        ..global.clone()
    };
    let mut edited = resumed.clone();
    edited.permission_mode = slide_builder::config::PermissionMode::Plan;
    let mut expected = global.clone();
    expected.permission_mode = edited.permission_mode;
    assert_eq!(configuration_for_save(&edited, &resumed, &global), expected);
    edited.model = "explicit-model".into();
    assert_eq!(configuration_for_save(&edited, &resumed, &global), edited);
}

#[tokio::test]
async fn completed_turn_reopens_ui_and_sdk_history_without_replaying_tools() {
    let directory = tempfile::tempdir().unwrap();
    let config = Config {
        provider: "scripted".into(),
        auth: "api-key".into(),
        model: "test".into(),
        ..Config::default()
    };
    let initial = provider("first answer");
    let agent = AgentHandle::new(
        Rho::builder()
            .provider(initial)
            .system_prompt(SystemPrompt::Custom("old deck context".into()))
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    let path = directory.path().join("application.sqlite3");
    let store = SessionStore::open(&path).unwrap();
    let mut saved = store
        .create(
            agent.snapshot(),
            initial_state(
                &directory.path().join("deck.pptx"),
                directory.path(),
                &config,
            ),
        )
        .unwrap();
    let id = saved.id.clone();
    let mut app = App::default();
    app.transcript
        .push(TranscriptItem::Message(slide_builder::tui::Message {
            role: slide_builder::tui::Role::User,
            text: "first question".into(),
            complete: true,
        }));
    let (tx, mut rx) = mpsc::unbounded_channel();
    agent.send("first question".into(), None, tx).await.unwrap();
    while let Ok(event) = rx.try_recv() {
        app.apply(event);
    }
    app.preview.active = 2;
    app.input.text = "draft edit".into();
    app.input.attach_active_slide = true;
    app.design_name = "Selected design".into();
    let pending = Some("design instructions".to_owned());
    checkpoint(&store, &mut saved, &agent, &app, &config, &pending).unwrap();
    let previous = saved.snapshot.clone();
    drop(store);
    drop(agent);
    let reopened = SessionStore::open(&path).unwrap();
    let saved = reopened.load(&id).unwrap();
    assert_eq!(saved.snapshot, previous);
    let mut restored_app = App::default();
    restore_app(&mut restored_app, &saved.state, 2);
    assert_eq!(restored_app.transcript, app.transcript);
    assert_eq!(restored_app.preview.active, 1);
    assert_eq!(restored_app.input.text, "draft edit");
    assert!(restored_app.input.attach_active_slide);
    assert_eq!(saved.state.pending_design_context, pending);
    assert_eq!(saved.state.provider, config.provider);
    assert_eq!(saved.state.auth, config.auth);
    assert_eq!(saved.state.model, config.model);
    let next = provider("second answer");
    let restored = AgentHandle::restore(
        Rho::builder().provider(next.clone()).build().unwrap(),
        saved.snapshot,
        "fresh deck context".into(),
    )
    .await
    .unwrap();
    assert!(next.recorded_requests().is_empty());
    assert_eq!(restored.snapshot().session_id().to_string(), id);
    let (tx, _) = mpsc::unbounded_channel();
    restored
        .send("second question".into(), None, tx)
        .await
        .unwrap();
    let requests = next.recorded_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].messages[0],
        Message::System("fresh deck context".into())
    );
    let text = serde_json::to_string(&requests[0].messages).unwrap();
    assert!(text.contains("first question"));
    assert!(text.contains("first answer"));
    assert!(text.contains("second question"));
    assert!(!text.contains("old deck context"));
}

#[tokio::test]
async fn shutdown_waits_for_cancelled_ui_tool_and_checkpoint_can_continue() {
    let directory = tempfile::tempdir().unwrap();
    let provider = ScriptedProvider::new(
        ModelIdentity::new("scripted", "scripted", "test"),
        [ScriptedTurn::completed(ModelResponse::Assistant(vec![
            ContentBlock::ToolCall(ToolCall {
                id: "select-1".into(),
                name: "set_active_slide".into(),
                arguments: serde_json::json!({"index": 1}),
            }),
        ]))],
    );
    let (commands, mut receiver) = mpsc::unbounded_channel();
    let agent = AgentHandle::new(
        Rho::builder()
            .provider(provider)
            .tool(UiTool::set_active(commands))
            .build()
            .unwrap(),
    )
    .await
    .unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let handle = agent.clone();
    let task = tokio::spawn(async move { handle.send("select slide".into(), None, tx).await });
    let UiToolCommand::SetActiveSlide { response, .. } = receiver.recv().await.unwrap() else {
        panic!("expected set active");
    };
    // Keep the response alive: cooperative cancellation must settle the tool
    // without depending on the UI dropping its pending reply.
    agent.request_shutdown();
    task.await.unwrap().unwrap();
    drop(response);
    assert!(!agent.is_active());
    let mut app = App::default();
    let mut cancelled = 0;
    while let Ok(event) = rx.try_recv() {
        if matches!(&event, AppEvent::Run(AgentEvent::RunCancelled)) {
            cancelled += 1;
        }
        app.apply(event);
    }
    assert_eq!(cancelled, 1);
    assert!(!app.run_active);
    let store = SessionStore::open(&directory.path().join("app.sqlite3")).unwrap();
    let config = Config::default();
    let mut saved = store
        .create(
            agent.snapshot(),
            initial_state(directory.path(), directory.path(), &config),
        )
        .unwrap();
    checkpoint(&store, &mut saved, &agent, &app, &config, &None).unwrap();
    let next = super::tests::provider("continued after cancellation");
    let restored = AgentHandle::restore(
        Rho::builder().provider(next.clone()).build().unwrap(),
        store.load(&saved.id).unwrap().snapshot,
        "current context".into(),
    )
    .await
    .unwrap();
    assert!(next.recorded_requests().is_empty());
    let (tx, _) = mpsc::unbounded_channel();
    restored.send("continue".into(), None, tx).await.unwrap();
    assert_eq!(next.recorded_requests().len(), 1);
}

#[tokio::test]
async fn failed_run_has_one_terminal_event_and_shutdown_before_start_is_safe() {
    let exhausted = ScriptedProvider::new(ModelIdentity::new("scripted", "scripted", "test"), []);
    let agent = AgentHandle::new(Rho::builder().provider(exhausted).build().unwrap())
        .await
        .unwrap();
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Same wrapper used by the interactive loop.
    if let Err(error) = agent.send("fail".into(), None, tx.clone()).await {
        tx.send(AppEvent::Run(AgentEvent::RunFailed(error.to_string())))
            .unwrap();
    }
    let mut terminals = 0;
    while let Ok(event) = rx.try_recv() {
        if matches!(
            event,
            AppEvent::Run(
                AgentEvent::RunFinished | AgentEvent::RunCancelled | AgentEvent::RunFailed(_)
            )
        ) {
            terminals += 1;
        }
    }
    assert_eq!(terminals, 1);
    agent.request_shutdown();
    assert!(agent.send("must not start".into(), None, tx).await.is_err());
    assert!(!agent.is_active());
}

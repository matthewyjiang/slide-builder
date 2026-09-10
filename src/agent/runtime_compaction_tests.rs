use super::*;
use rho_sdk::{
    model::{Message, ModelIdentity, ModelResponse},
    provider::{ModelProvider, ScriptedProvider, ScriptedTurn},
    Rho,
};

#[tokio::test]
async fn tool_output_triggers_compaction_before_the_next_model_step() {
    use rho_sdk::{
        model::{context::estimate_context_tokens, ToolCall, ToolSpec},
        tool::{Tool, ToolContext, ToolFuture, ToolInvocation, ToolOutput},
    };
    struct Inspect(String);
    impl Tool for Inspect {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "inspect".into(),
                description: "Inspect the deck".into(),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            }
        }
        fn call<'a>(&'a self, _: ToolInvocation, _: ToolContext) -> ToolFuture<'a> {
            Box::pin(async move { Ok(ToolOutput::text(self.0.clone())) })
        }
    }
    let tool = Inspect("slide details ".repeat(1_000));
    let specs = vec![tool.spec()];
    let history = vec![
        Message::System("system".into()),
        Message::user_text("old context ".repeat(1_000)),
    ];
    // Size the fixture window from its actual context estimate. Initial context
    // fits, but the comparably sized inspection result crosses the 85% threshold.
    let window = estimate_context_tokens(&history, &specs) * 2;
    let call = ToolCall {
        id: "inspection".into(),
        name: "inspect".into(),
        arguments: serde_json::json!({}),
    };
    let provider = ScriptedProvider::new(
        ModelIdentity::new("test", "test", "model"),
        [
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::ToolCall(
                call.clone(),
            )])),
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::Text(
                "remembered".into(),
            )])),
            ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::Text(
                "continued".into(),
            )])),
        ],
    );
    let rho = Rho::builder()
        .provider(provider.clone())
        .tool(tool)
        .compactor(ModelCompactor {
            provider: Arc::new(provider.clone()),
            tools: specs.clone(),
            window: Some(window),
        })
        .compaction_policy(compaction::policy(Some(window)).unwrap())
        .build()
        .unwrap();
    let agent = AgentHandle::with_options(rho.into(), SessionOptions::default().history(history))
        .await
        .unwrap();
    let (events, mut receiver) = mpsc::unbounded_channel();
    agent
        .send("inspect then continue".into(), None, events)
        .await
        .unwrap();
    let requests = provider.recorded_requests();
    assert_eq!(requests.len(), 3);
    assert!(estimate_context_tokens(&requests[0].messages, &specs) < window * 85 / 100);
    assert!(requests[1].tools.is_empty());
    let next = &requests[2].messages;
    let start = next
        .iter()
        .position(|message| {
            message
                .completed_assistant_content()
                .is_some_and(|blocks| blocks.contains(&ContentBlock::ToolCall(call.clone())))
        })
        .unwrap();
    assert!(
        matches!(&next[start + 1], Message::ToolResult(result) if result.id == "inspection" && result.content == "slide details ".repeat(1_000))
    );
    let mut tool_finished = false;
    while let Ok(event) = receiver.try_recv() {
        match event {
            AppEvent::Run(AgentEvent::ToolFinished { .. }) => tool_finished = true,
            AppEvent::Run(AgentEvent::CompactionStarted) => assert!(tool_finished),
            _ => {}
        }
    }
    assert_eq!(agent.snapshot().compaction().completed_compactions(), 1);
}

#[tokio::test]
async fn model_switch_updates_the_summary_provider_and_context_threshold() {
    struct NativeCapable(ScriptedProvider);
    impl ModelProvider for NativeCapable {
        fn identity(&self) -> ModelIdentity {
            self.0.identity()
        }
        fn send_turn<'a>(
            &'a self,
            request: rho_sdk::model::ModelRequest<'a>,
        ) -> rho_sdk::provider::ProviderFuture<'a> {
            self.0.send_turn(request)
        }
        fn native_compact<'a>(
            &'a self,
            _: rho_sdk::model::ModelRequest<'a>,
        ) -> Option<rho_sdk::provider::NativeCompactionFuture<'a>> {
            Some(Box::pin(async {
                panic!("provider-bound native compaction must not be requested")
            }))
        }
    }
    for window in [Some(100_000), Some(2_000), None] {
        let old = ScriptedProvider::new(ModelIdentity::new("old", "test", "old"), []);
        let rho = Rho::builder().provider(old.clone()).build().unwrap();
        let history = vec![
            Message::System("system".into()),
            Message::user_text("old context ".repeat(1_000)),
            Message::assistant_text("old answer ".repeat(1_000)),
        ];
        let agent = AgentHandle::with_options(
            rho.into(),
            SessionOptions::default().history(history.clone()),
        )
        .await
        .unwrap();
        let texts = if window == Some(2_000) {
            vec!["summary", "continued"]
        } else {
            vec!["continued"]
        };
        let new = ScriptedProvider::new(
            ModelIdentity::new("new", "test", "new"),
            texts.iter().map(|text| {
                ScriptedTurn::completed(ModelResponse::Assistant(vec![ContentBlock::Text(
                    (*text).into(),
                )]))
            }),
        );
        agent.active.store(true, Ordering::Release);
        assert!(agent.replace_model(Arc::new(new.clone()), window).is_err());
        assert_eq!(agent.session.history(), history);
        assert_eq!(agent.snapshot().provider(), &old.identity());
        agent.active.store(false, Ordering::Release);
        agent
            .replace_model(Arc::new(NativeCapable(new.clone())), window)
            .unwrap();
        assert_eq!(agent.session.history(), history);
        let (events, _) = mpsc::unbounded_channel();
        agent
            .send("latest edit".into(), None, events)
            .await
            .unwrap();
        assert!(old.recorded_requests().is_empty());
        assert_eq!(new.recorded_requests().len(), texts.len());
        assert_eq!(
            agent.snapshot().compaction().completed_compactions(),
            u64::from(window == Some(2_000))
        );
        if window == Some(2_000) {
            let compacted = agent.session.history();
            let switched = ScriptedProvider::new(
                ModelIdentity::new("third-provider", "test", "third-model"),
                [ScriptedTurn::completed(ModelResponse::Assistant(vec![
                    ContentBlock::Text("switched".into()),
                ]))],
            );
            agent
                .replace_model(Arc::new(switched.clone()), None)
                .unwrap();
            assert_eq!(agent.session.history(), compacted);
            let (events, _) = mpsc::unbounded_channel();
            agent
                .send("continue after switch".into(), None, events)
                .await
                .unwrap();
            let mut expected = compacted;
            expected.push(Message::user_text("continue after switch"));
            assert_eq!(switched.recorded_requests()[0].messages, expected);
            assert!(expected.contains(&Message::user_text(
                "Summary of earlier conversation for model context only:\n\nsummary"
            )));
        }
    }
}

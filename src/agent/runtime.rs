use super::compaction::{self, ModelCompactor};
use super::runtime_builder::build_provider;
pub use super::runtime_builder::{build_rho, register_deck_tools, ConfiguredRuntime};
use crate::agent::tool_summary;
use crate::tui::{AgentEvent, AppEvent};
use anyhow::Result;
use rho_sdk::{
    model::{ContentBlock, ImageContent},
    Session, SessionOptions, UserInput,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Owns a rho session and exposes a cancellation handle independently of the
/// task that is draining the active run.
#[derive(Clone)]
pub struct AgentHandle {
    session: Session,
    tools: Vec<rho_sdk::model::ToolSpec>,
    active: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
    cancel_requested: Arc<AtomicBool>,
    cancellation: Arc<Mutex<Option<rho_sdk::CancellationToken>>>,
}
impl AgentHandle {
    pub async fn new(rho: impl Into<ConfiguredRuntime>) -> Result<Self> {
        Self::with_options(rho.into(), SessionOptions::default()).await
    }

    /// Restore history without starting a run or replaying any tool. Replace the
    /// saved system prompt with current deck/workspace context before rebinding.
    pub async fn restore(
        rho: impl Into<ConfiguredRuntime>,
        snapshot: rho_sdk::SessionSnapshot,
        prompt: String,
    ) -> Result<Self> {
        let mut history = snapshot.history().to_vec();
        match history.first_mut() {
            Some(rho_sdk::model::Message::System(saved)) => *saved = prompt,
            _ => history.insert(0, rho_sdk::model::Message::System(prompt)),
        }
        Self::with_options(
            rho.into(),
            SessionOptions::from_snapshot(snapshot).history(history),
        )
        .await
    }

    async fn with_options(runtime: ConfiguredRuntime, options: SessionOptions) -> Result<Self> {
        Ok(Self {
            session: runtime.rho.session(options).await?,
            tools: runtime.tools,
            active: Arc::new(AtomicBool::new(false)),
            closing: Arc::new(AtomicBool::new(false)),
            cancel_requested: Arc::new(AtomicBool::new(false)),
            cancellation: Arc::new(Mutex::new(None)),
        })
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    /// Reject future turns and cancel even if the current task is still starting.
    pub fn request_shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        self.cancel();
    }
    pub fn cancel(&self) -> bool {
        // Escape may arrive between spawning send() and installing the SDK token.
        self.cancel_requested.store(true, Ordering::Release);
        let cancellation = self
            .cancellation
            .lock()
            .expect("agent cancellation mutex poisoned")
            .clone();
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
            true
        } else {
            false
        }
    }
    pub async fn send(
        &self,
        text: String,
        image_path: Option<PathBuf>,
        events: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        if self.closing.load(Ordering::Acquire) {
            anyhow::bail!("session is shutting down");
        }
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            anyhow::bail!("a run is already active")
        }
        let result = self.run(text, image_path, events).await;
        *self
            .cancellation
            .lock()
            .expect("agent cancellation mutex poisoned") = None;
        self.cancel_requested.store(false, Ordering::Release);
        self.active.store(false, Ordering::Release);
        result
    }
    async fn run(
        &self,
        text: String,
        image_path: Option<PathBuf>,
        events: mpsc::UnboundedSender<AppEvent>,
    ) -> Result<()> {
        let input = if let Some(path) = image_path {
            let bytes = std::fs::read(&path)?;
            if bytes.len() > 32 * 1024 * 1024 {
                anyhow::bail!("active slide image exceeds the 32 MiB attachment limit");
            }
            let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
            UserInput::text_and_images(
                text,
                [ImageContent {
                    data,
                    mime_type: "image/png".into(),
                }],
            )
        } else {
            UserInput::text(text)
        };
        let (source, mut boundaries) = rho_sdk::boundary_input_channel();
        self.session.set_boundary_inputs(Some(source))?;
        let mut run = self.session.start(input).await?;
        *self
            .cancellation
            .lock()
            .expect("agent cancellation mutex poisoned") = Some(run.cancellation_handle());
        if self.closing.load(Ordering::Acquire) || self.cancel_requested.load(Ordering::Acquire) {
            self.cancel();
        }
        let mut pending = Vec::new();
        let mut terminal_events = Vec::new();
        loop {
            tokio::select! {
                // ToolFinished events precede the boundary request. Drain them
                // first so their assets reach the very next provider request.
                biased;
                event = run.next_event() => {
                    let Some(event) = event else { break };
                    // Publish the terminal transition only after the worker has
                    // settled. Failure then has one source, never a second event
                    // queued behind the next user turn.
                    if matches!(&event, rho_sdk::RunEvent::Completed { .. } | rho_sdk::RunEvent::Cancelled { .. } | rho_sdk::RunEvent::Failed { .. }) {
                        terminal_events = adapt_run_event(event);
                        continue;
                    }
                    if let rho_sdk::RunEvent::ToolFinished {
                        call_id,
                        result: rho_sdk::ToolCompletion::Success(output),
                    } = &event {
                        let images: Vec<_> = output.presentation().assets().iter()
                            .filter(|asset| asset.media_type().starts_with("image/"))
                            .map(|asset| ContentBlock::Image(ImageContent {
                                data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, asset.bytes()),
                                mime_type: asset.media_type().to_owned(),
                            })).collect();
                        if !images.is_empty() {
                            pending.push(ContentBlock::Text(format!(
                                "Internal tool image feedback for call {call_id}. Images follow in the order listed in the tool result.\n{}",
                                output.content(),
                            )));
                            pending.extend(images);
                        }
                    }
                    for event in adapt_run_event(event) {
                        let _ = events.send(event);
                    }
                }
                Some(request) = boundaries.recv() => {
                    let input = if pending.is_empty() { None } else {
                        Some(UserInput::content(std::mem::take(&mut pending))?)
                    };
                    request.respond(input).await;
                }
            }
        }
        match run.outcome().await {
            Ok(_) => {}
            Err(rho_sdk::Error::Cancelled) => {
                terminal_events = vec![AppEvent::Run(AgentEvent::RunCancelled)];
            }
            Err(error) => return Err(error.into()),
        }
        for event in terminal_events {
            let _ = events.send(event);
        }
        Ok(())
    }
    pub fn snapshot(&self) -> rho_sdk::SessionSnapshot {
        self.session.snapshot()
    }

    /// Swaps the session onto another provider/model without losing history.
    /// Fails if a run is active; callers should check `is_active` first.
    pub fn replace_provider(&self, provider: &str, auth: &str, model: &str) -> Result<()> {
        let window = compaction::context_window(provider, model);
        let provider = build_provider(provider, auth, model)?;
        self.replace_model(provider, window)
    }

    fn replace_model(
        &self,
        provider: Arc<dyn rho_sdk::provider::ModelProvider>,
        window: Option<u64>,
    ) -> Result<()> {
        // Share the same guard as send(), so a turn cannot start between the two
        // idle-only SDK updates. No history or model changes occur on rejection.
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            anyhow::bail!("a run is already active");
        }
        let result = (|| {
            self.session.replace_provider(provider.clone())?;
            // The session is private and every run starts under the active guard.
            // In the pinned SDK, set_compaction can fail only for an active run
            // or a policy without a compactor. Neither is possible here, so this
            // second update cannot leave a new provider with stale compaction.
            self.session.set_compaction(
                Some(Arc::new(ModelCompactor {
                    provider,
                    tools: self.tools.clone(),
                    window,
                })),
                compaction::policy(window),
            )?;
            Ok(())
        })();
        self.active.store(false, Ordering::Release);
        result
    }
}

/// Translate SDK values at the integration boundary so the TUI remains
/// independent of rho-sdk.
pub fn adapt_run_event(event: rho_sdk::RunEvent) -> Vec<AppEvent> {
    use rho_sdk::{RunEvent, ToolCompletion};
    let event = match event {
        RunEvent::Started { run_id, .. } => AppEvent::RunHandleReady {
            run_id: run_id.to_string(),
        },
        RunEvent::AssistantTextDelta { text } => AppEvent::Run(AgentEvent::TextDelta(text)),
        RunEvent::CompactionStarted { .. } => AppEvent::Run(AgentEvent::CompactionStarted),
        RunEvent::CompactionCompleted { outcome, .. } => {
            AppEvent::Run(AgentEvent::CompactionCompleted {
                previous_tokens: outcome.previous_tokens(),
                current_tokens: outcome.current_tokens(),
            })
        }
        RunEvent::ToolProposed { call } => {
            let summary = tool_summary::target(&call.name, &call.arguments);
            let arguments = serde_json::to_string_pretty(&call.arguments)
                .expect("tool argument JSON values are serializable");
            AppEvent::Run(AgentEvent::ToolProposed {
                id: call.id,
                name: call.name,
                summary,
                arguments,
            })
        }
        RunEvent::ToolStarted { call_id, .. } => AppEvent::Run(AgentEvent::ToolStarted {
            id: call_id.to_string(),
        }),
        RunEvent::ToolUpdated {
            call_id, progress, ..
        } => AppEvent::Run(AgentEvent::ToolUpdated {
            id: call_id.to_string(),
            detail: progress.text().to_owned(),
        }),
        RunEvent::ToolFinished { call_id, result } => {
            let result = match result {
                ToolCompletion::Success(output) => Ok(output.content().to_owned()),
                ToolCompletion::Failure(error) => Err(error.message().to_owned()),
                ToolCompletion::Unavailable => Err("tool unavailable".into()),
                _ => Err("unknown tool completion".into()),
            };
            AppEvent::Run(AgentEvent::ToolFinished {
                id: call_id.to_string(),
                result,
            })
        }
        RunEvent::Completed { .. } => {
            return vec![
                AppEvent::Run(AgentEvent::MessageFinished),
                AppEvent::Run(AgentEvent::RunFinished),
            ];
        }
        RunEvent::Cancelled { .. } => AppEvent::Run(AgentEvent::RunCancelled),
        RunEvent::Failed { message, .. } => AppEvent::Run(AgentEvent::RunFailed(message)),
        _ => return vec![],
    };
    vec![event]
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "runtime_compaction_tests.rs"]
mod compaction_tests;

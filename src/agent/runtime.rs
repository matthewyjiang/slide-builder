use crate::agent::{
    deck_engine::DeckEngine,
    skill_tool::LoadSkillTool,
    tool_summary,
    tools::{UiTool, UiToolCommand},
};
use crate::skills::Skill;
use crate::tui::{AgentEvent, AppEvent};
use anyhow::{Context, Result};
use rho_sdk::{
    approval_channel,
    model::{ContentBlock, ImageContent},
    ApprovalRequestReceiver, Rho, Session, SessionOptions, SystemPrompt, UserInput, Workspace,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

/// Keep complex deck-building turns from hitting rho-sdk's small safety default.
/// This mirrors Rho's interactive application while still bounding runaway loops.
fn run_step_limit() -> NonZeroUsize {
    NonZeroUsize::new(10_000).expect("step limit is nonzero")
}

/// Owns a rho session and exposes a cancellation handle independently of the
/// task that is draining the active run.
#[derive(Clone)]
pub struct AgentHandle {
    session: Session,
    active: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
    cancel_requested: Arc<AtomicBool>,
    cancellation: Arc<Mutex<Option<rho_sdk::CancellationToken>>>,
}
impl AgentHandle {
    pub async fn new(rho: Rho) -> Result<Self> {
        Self::with_options(rho, SessionOptions::default()).await
    }

    /// Restore history without starting a run or replaying any tool. Replace the
    /// saved system prompt with current deck/workspace context before rebinding.
    pub async fn restore(
        rho: Rho,
        snapshot: rho_sdk::SessionSnapshot,
        prompt: String,
    ) -> Result<Self> {
        let mut history = snapshot.history().to_vec();
        match history.first_mut() {
            Some(rho_sdk::model::Message::System(saved)) => *saved = prompt,
            _ => history.insert(0, rho_sdk::model::Message::System(prompt)),
        }
        Self::with_options(
            rho,
            SessionOptions::from_snapshot(snapshot).history(history),
        )
        .await
    }

    async fn with_options(rho: Rho, options: SessionOptions) -> Result<Self> {
        Ok(Self {
            session: rho.session(options).await?,
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
        let provider = build_provider(provider, auth, model)?;
        self.session.replace_provider(provider)?;
        Ok(())
    }
}

/// Builds the rho model provider using slide-builder's own credential store.
fn build_provider(
    provider: &str,
    auth: &str,
    model: &str,
) -> Result<std::sync::Arc<dyn rho_sdk::provider::ModelProvider>> {
    let options = rho_providers::ProviderBuildOptions::new(provider, model, DEFAULT_REASONING)
        .map_err(anyhow::Error::new)
        .context("provider configuration failed")?
        .with_auth(auth)
        .map_err(anyhow::Error::new)
        .context("provider authentication mode failed")?;
    let credentials = rho_providers::auth::provider_credentials::ApplicationCredentialSource::new(
        std::sync::Arc::new(crate::credentials::SlideCredentialStore),
    );
    rho_providers::build_sdk_provider_with_source(options, &credentials)
        .map_err(anyhow::Error::new)
        .context("provider setup failed; log in from slide-builder setup")
}

const DEFAULT_REASONING: rho_sdk::ReasoningLevel = rho_sdk::ReasoningLevel::Medium;

/// Translate SDK values at the integration boundary so the TUI remains
/// independent of rho-sdk.
pub fn adapt_run_event(event: rho_sdk::RunEvent) -> Vec<AppEvent> {
    use rho_sdk::{RunEvent, ToolCompletion};
    let event = match event {
        RunEvent::Started { run_id, .. } => AppEvent::RunHandleReady {
            run_id: run_id.to_string(),
        },
        RunEvent::AssistantTextDelta { text } => AppEvent::Run(AgentEvent::TextDelta(text)),
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

#[allow(clippy::too_many_arguments)]
pub fn build_rho(
    provider: &str,
    auth: &str,
    model: &str,
    prompt: String,
    repo: &Path,
    decks: &Path,
    design: Option<&Path>,
    skills: &[Skill],
    ui_tools: mpsc::UnboundedSender<UiToolCommand>,
    engine: DeckEngine,
    policy: crate::agent::policy::SlidePolicy,
) -> Result<(Rho, ApprovalRequestReceiver)> {
    let provider = build_provider(provider, auth, model)?;
    let mut workspace = Workspace::new(repo)?.with_granted_root(decks)?;
    if let Some(path) = design {
        workspace = workspace.with_granted_root(path)?;
    }
    let (approvals, receiver) = approval_channel(NonZeroUsize::new(16).unwrap());
    let mut builder = Rho::builder()
        .provider_shared(provider)
        .system_prompt(SystemPrompt::Custom(prompt))
        .workspace(workspace)
        .workspace_policy(policy)
        .approval_handler(approvals)
        .reasoning_level(DEFAULT_REASONING)
        .max_steps(run_step_limit());
    for tool in rho_agent_tools::coding_tools(rho_agent_tools::CodingToolOptions::new()) {
        builder = builder.tool_shared(tool)
    }
    builder = builder.tool_shared(rho_agent_tools::shell_tool(
        rho_agent_tools::ShellToolOptions::new()
            .max_output_bytes(rho_agent_tools::DEFAULT_MAX_OUTPUT_BYTES),
    ));
    builder = builder.tool(LoadSkillTool::new(skills.to_vec()));
    builder = builder.tool(UiTool::render(ui_tools.clone()));
    builder = builder.tool(UiTool::set_active(ui_tools));
    builder = register_deck_tools(builder, engine);
    Ok((builder.build()?, receiver))
}

pub fn register_deck_tools(
    mut builder: rho_sdk::RhoBuilder,
    engine: DeckEngine,
) -> rho_sdk::RhoBuilder {
    for tool in crate::agent::deck_tools::semantic_tools(engine.clone()) {
        builder = builder.tool_shared(tool)
    }
    for tool in crate::agent::asset_tools::tools(engine) {
        builder = builder.tool_shared(tool)
    }
    builder
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

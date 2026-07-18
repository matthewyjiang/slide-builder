use crate::agent::deck_engine::DeckEngine;
use crate::tui::{AgentEvent, AppEvent};
use anyhow::{Context, Result};
use rho_sdk::{
    approval_channel, model::ImageContent, ApprovalRequestReceiver, Rho, Session, SessionOptions,
    SystemPrompt, UserInput, Workspace,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

/// Owns a rho session and exposes a cancellation handle independently of the
/// task that is draining the active run.
#[derive(Clone)]
pub struct AgentHandle {
    session: Session,
    active: Arc<AtomicBool>,
    cancellation: Arc<Mutex<Option<rho_sdk::CancellationToken>>>,
}
impl AgentHandle {
    pub async fn new(rho: Rho) -> Result<Self> {
        Ok(Self {
            session: rho.session(SessionOptions::default()).await?,
            active: Arc::new(AtomicBool::new(false)),
            cancellation: Arc::new(Mutex::new(None)),
        })
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn cancel(&self) -> bool {
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
        let mut run = self.session.start(input).await?;
        *self
            .cancellation
            .lock()
            .expect("agent cancellation mutex poisoned") = Some(run.cancellation_handle());
        while let Some(event) = run.next_event().await {
            for event in adapt_run_event(event) {
                let _ = events.send(event);
            }
        }
        Ok(())
    }
    pub fn snapshot(&self) -> rho_sdk::SessionSnapshot {
        self.session.snapshot()
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
        RunEvent::ToolProposed { call } => AppEvent::Run(AgentEvent::ToolProposed {
            id: call.id,
            name: call.name,
            summary: call.arguments.to_string(),
        }),
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
        RunEvent::Cancelled { .. } => AppEvent::Run(AgentEvent::RunFinished),
        RunEvent::Failed { message, .. } => AppEvent::Run(AgentEvent::RunFailed(message)),
        _ => return vec![],
    };
    vec![event]
}

#[allow(clippy::too_many_arguments)]
pub fn build_rho(
    provider: &str,
    model: &str,
    prompt: String,
    repo: &Path,
    decks: &Path,
    design: Option<&Path>,
    engine: DeckEngine,
    policy: crate::agent::policy::SlidePolicy,
) -> Result<(Rho, ApprovalRequestReceiver)> {
    let reasoning = rho_sdk::ReasoningLevel::Medium;
    let options = rho_providers::ProviderBuildOptions::new(provider, model, reasoning)
        .map_err(anyhow::Error::new)
        .context("provider configuration failed")?;
    let credentials = rho_providers::auth::provider_credentials::ApplicationCredentialSource::new(
        std::sync::Arc::new(crate::credentials::SlideCredentialStore),
    );
    let provider = rho_providers::build_sdk_provider_with_source(options, &credentials)
        .map_err(anyhow::Error::new)
        .context("provider setup failed; log in from slide-builder setup")?;
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
        .reasoning_level(reasoning);
    for tool in rho_agent_tools::coding_tools(rho_agent_tools::CodingToolOptions::new()) {
        builder = builder.tool_shared(tool)
    }
    builder = builder.tool_shared(rho_agent_tools::shell_tool(
        rho_agent_tools::DEFAULT_MAX_OUTPUT_BYTES,
    ));
    builder = register_deck_tools(builder, engine);
    Ok((builder.build()?, receiver))
}

pub fn register_deck_tools(
    mut builder: rho_sdk::RhoBuilder,
    engine: DeckEngine,
) -> rho_sdk::RhoBuilder {
    for tool in crate::agent::deck_tools::semantic_tools(engine) {
        builder = builder.tool_shared(tool)
    }
    builder
}

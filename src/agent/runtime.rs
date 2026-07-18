use crate::agent::deck_engine::DeckEngine;
use anyhow::Result;
use rho_sdk::{
    approval_channel, ApprovalRequestReceiver, Rho, Session, SessionOptions, SystemPrompt,
    UserInput, Workspace,
};
use std::{num::NonZeroUsize, path::Path};
use tokio::sync::mpsc;

/// Owns a rho session and serializes user runs onto the unified UI event channel.
pub struct AgentHandle {
    session: Session,
    active: bool,
}
impl AgentHandle {
    pub async fn new(rho: Rho) -> Result<Self> {
        Ok(Self {
            session: rho.session(SessionOptions::default()).await?,
            active: false,
        })
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub async fn send(
        &mut self,
        text: String,
        events: mpsc::UnboundedSender<rho_sdk::RunEvent>,
    ) -> Result<()> {
        if self.active {
            anyhow::bail!("a run is already active")
        };
        self.active = true;
        let mut run = self.session.start(UserInput::text(text)).await?;
        while let Some(event) = run.next_event().await {
            let terminal = matches!(
                event,
                rho_sdk::RunEvent::Completed { .. }
                    | rho_sdk::RunEvent::Failed { .. }
                    | rho_sdk::RunEvent::Cancelled { .. }
            );
            let _ = events.send(event);
            if terminal {
                break;
            }
        }
        self.active = false;
        Ok(())
    }
    pub fn snapshot(&self) -> rho_sdk::SessionSnapshot {
        self.session.snapshot()
    }
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
    let provider = rho_providers::build_sdk_provider(provider, model, reasoning)
        .map_err(|e| anyhow::anyhow!("provider setup failed: {e}; run `rho login`"))?;
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

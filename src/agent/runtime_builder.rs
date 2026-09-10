//! Provider authentication and construction of the deck-building runtime.
use super::{
    compaction::{self, ModelCompactor},
    deck_engine::DeckEngine,
    skill_tool::LoadSkillTool,
    tools::{UiTool, UiToolCommand},
};
use crate::skills::Skill;
use anyhow::{Context, Result};
use rho_sdk::{approval_channel, ApprovalRequestReceiver, Rho, SystemPrompt, Workspace};
use std::{num::NonZeroUsize, path::Path, sync::Arc};
use tokio::sync::mpsc;

pub(super) const DEFAULT_REASONING: rho_sdk::ReasoningLevel = rho_sdk::ReasoningLevel::Medium;

/// Runtime plus advertised tools needed to budget compaction after model switches.
pub struct ConfiguredRuntime {
    pub(super) rho: Rho,
    pub(super) tools: Vec<rho_sdk::model::ToolSpec>,
}

impl From<Rho> for ConfiguredRuntime {
    fn from(rho: Rho) -> Self {
        Self {
            rho,
            tools: Vec::new(),
        }
    }
}

/// Builds the rho model provider using slide-builder's own credential store.
pub(super) fn build_provider(
    provider: &str,
    auth: &str,
    model: &str,
) -> Result<Arc<dyn rho_sdk::provider::ModelProvider>> {
    let options = rho_providers::ProviderBuildOptions::new(provider, model, DEFAULT_REASONING)
        .map_err(anyhow::Error::new)
        .context("provider configuration failed")?
        .with_auth(auth)
        .map_err(anyhow::Error::new)
        .context("provider authentication mode failed")?;
    let credentials = rho_providers::auth::provider_credentials::ApplicationCredentialSource::new(
        Arc::new(crate::credentials::SlideCredentialStore),
    );
    rho_providers::build_sdk_provider_with_source(options, &credentials)
        .map_err(anyhow::Error::new)
        .context("provider setup failed; log in from slide-builder setup")
}

/// Keep deck-building turns from hitting the SDK's small safety default.
/// This mirrors Rho's interactive application while bounding runaway loops.
fn run_step_limit() -> NonZeroUsize {
    NonZeroUsize::new(10_000).expect("step limit is nonzero")
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
) -> Result<(ConfiguredRuntime, ApprovalRequestReceiver)> {
    let window = compaction::context_window(provider, model);
    let provider = build_provider(provider, auth, model)?;
    let mut workspace = Workspace::new(repo)?.with_granted_root(decks)?;
    if let Some(path) = design {
        workspace = workspace.with_granted_root(path)?;
    }
    let (approvals, receiver) = approval_channel(NonZeroUsize::new(16).unwrap());
    let mut builder = Rho::builder()
        .provider_shared(provider.clone())
        .system_prompt(SystemPrompt::Custom(prompt))
        .workspace(workspace)
        .workspace_policy(policy)
        .approval_handler(approvals)
        .reasoning_level(DEFAULT_REASONING)
        .max_steps(run_step_limit());
    let mut tools = rho_agent_tools::coding_tools(rho_agent_tools::CodingToolOptions::new());
    tools.push(rho_agent_tools::shell_tool(
        rho_agent_tools::ShellToolOptions::new()
            .max_output_bytes(rho_agent_tools::DEFAULT_MAX_OUTPUT_BYTES),
    ));
    tools.push(Arc::new(LoadSkillTool::new(skills.to_vec())));
    tools.push(Arc::new(UiTool::render(ui_tools.clone())));
    tools.push(Arc::new(UiTool::set_active(ui_tools)));
    tools.extend(crate::agent::deck_tools::semantic_tools(engine.clone()));
    tools.extend(crate::agent::asset_tools::tools(engine));
    let specs: Vec<_> = tools.iter().map(|tool| tool.spec()).collect();
    for tool in tools {
        builder = builder.tool_shared(tool);
    }
    builder = builder.compactor(ModelCompactor {
        provider,
        tools: specs.clone(),
        window,
    });
    if let Some(policy) = compaction::policy(window) {
        builder = builder.compaction_policy(policy);
    }
    Ok((
        ConfiguredRuntime {
            rho: builder.build()?,
            tools: specs,
        },
        receiver,
    ))
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

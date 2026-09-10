//! Model-backed compaction policy. The SDK owns history commits and cancellation.
use super::compaction_history::{partition, summary_request};
use rho_sdk::{
    model::{
        context::estimate_context_tokens, ContentBlock, ModelRequest, ModelResponse, ToolSpec,
    },
    provider::ModelProvider,
    CompactionFuture, CompactionOutput, CompactionPolicy, CompactionRequest, Compactor, Error,
};
use std::{num::NonZeroU64, sync::Arc};

pub(super) use crate::models::context_window;

pub(super) fn policy(window: Option<u64>) -> Option<CompactionPolicy> {
    // Product policy: start at 85% and aim for 50%, leaving room for the next turn.
    window
        .and_then(|window| NonZeroU64::new(window.saturating_mul(85).div_ceil(100)))
        .map(CompactionPolicy::at_context_tokens)
}

pub(super) struct ModelCompactor {
    pub provider: Arc<dyn ModelProvider>,
    pub tools: Vec<ToolSpec>,
    pub window: Option<u64>,
}

impl Compactor for ModelCompactor {
    fn compact<'a>(&'a self, request: CompactionRequest) -> CompactionFuture<'a> {
        Box::pin(async move {
            let cancellation = request.cancellation().clone();
            if cancellation.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let previous = estimate_context_tokens(request.messages(), &self.tools);
            let target = self.window.map_or(previous / 2, |window| window / 2);
            let Some(partition) = partition(request.messages(), &self.tools, target) else {
                return Err(Error::InvalidHostResponse { message: format!("Automatic compaction cannot reduce this context: estimated {previous} tokens, target {target}. There is no eligible older history to compact for this target while preserving the system prompt and recent exchange. Choose a larger-context model or start a new session.") });
            };
            // Native compaction can produce provider/model-bound opaque context.
            // Use text so summaries survive model switches and session restores.
            let messages = summary_request(&partition.older);
            let response = self
                .provider
                .send_turn(ModelRequest {
                    messages: &messages,
                    tools: &[],
                    cancellation: cancellation.clone(),
                    reasoning_level: super::runtime_builder::DEFAULT_REASONING,
                    prompt_cache_key: None,
                })
                .await;
            if cancellation.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let ModelResponse::Assistant(blocks) = response.map_err(|error| Error::InvalidHostResponse {
                message: format!("Automatic compaction failed: {error}. History is unchanged; retry or choose a larger-context model."),
            })?;
            let summary = blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text(text) => Some(text.as_str()),
                    ContentBlock::Image(_) | ContentBlock::ToolCall(_) => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if summary.trim().is_empty() {
                return Err(Error::InvalidHostResponse { message: "Automatic compaction returned an empty summary. History is unchanged; retry or choose another model.".into() });
            }
            let mut replacement = partition.leading;
            replacement.push(rho_sdk::model::Message::user_text(format!(
                "Summary of earlier conversation for model context only:\n\n{}",
                summary.trim()
            )));
            replacement.extend(partition.recent);
            let current = estimate_context_tokens(&replacement, &self.tools);
            if current >= previous {
                return Err(Error::InvalidHostResponse { message: format!("Automatic compaction did not reduce context: estimated {previous} tokens before, {current} after, target {target}. History is unchanged; choose a larger-context model or start a new session.") });
            }
            CompactionOutput::new(replacement)
        })
    }
}

#[cfg(test)]
#[path = "compaction_tests.rs"]
mod tests;

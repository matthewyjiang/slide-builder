//! Dedicated, one-shot agent for converting untrusted template evidence into design guidance.
//!
//! This agent intentionally has no tools, no active-deck context, and no shared conversation
//! history. The application owns all filesystem work and validates the generated document.

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use rho_sdk::{
    model::ImageContent, Rho, RunEvent, Session, SessionOptions, SystemPrompt, UserInput, Workspace,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_CONTACT_SHEET_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct DesignImporterDefinition;

impl DesignImporterDefinition {
    pub async fn create(
        self,
        provider: &str,
        model: &str,
        workspace_root: &Path,
    ) -> Result<DesignImporter> {
        let rho = self.build_runtime(provider, model, workspace_root)?;
        Ok(DesignImporter {
            session: rho.session(SessionOptions::default()).await?,
            cancellation: Arc::new(Mutex::new(None)),
        })
    }

    fn build_runtime(&self, provider: &str, model: &str, workspace_root: &Path) -> Result<Rho> {
        let reasoning = rho_sdk::ReasoningLevel::Medium;
        let options = rho_providers::ProviderBuildOptions::new(provider, model, reasoning)
            .map_err(anyhow::Error::new)
            .context("design importer provider configuration failed")?;
        let credentials =
            rho_providers::auth::provider_credentials::ApplicationCredentialSource::new(Arc::new(
                crate::credentials::SlideCredentialStore,
            ));
        let provider = rho_providers::build_sdk_provider_with_source(options, &credentials)
            .map_err(anyhow::Error::new)
            .context("design importer provider setup failed")?;
        Ok(Rho::builder()
            .provider_shared(provider)
            .system_prompt(SystemPrompt::Custom(
                crate::design_import::IMPORT_SYSTEM_PROMPT.to_owned(),
            ))
            .workspace(Workspace::new(workspace_root)?)
            .reasoning_level(reasoning)
            // Deliberately register no tools. Template evidence is untrusted.
            .build()?)
    }
}

#[derive(Clone)]
pub struct DesignImporter {
    session: Session,
    cancellation: Arc<Mutex<Option<rho_sdk::CancellationToken>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesignImporterOutcome {
    Completed(String),
    Cancelled,
}

impl DesignImporter {
    pub fn cancel(&self) -> bool {
        let cancellation = self
            .cancellation
            .lock()
            .expect("design importer cancellation mutex poisoned")
            .clone();
        if let Some(cancellation) = cancellation {
            cancellation.cancel();
            true
        } else {
            false
        }
    }

    pub async fn run(
        &self,
        prompt: String,
        contact_sheet: Option<PathBuf>,
    ) -> Result<DesignImporterOutcome> {
        let input = import_input(prompt, contact_sheet)?;
        let mut run = self.session.start(input).await?;
        *self
            .cancellation
            .lock()
            .expect("design importer cancellation mutex poisoned") =
            Some(run.cancellation_handle());

        let mut output = String::new();
        let result = loop {
            let Some(event) = run.next_event().await else {
                break Err(anyhow::anyhow!(
                    "design importer ended without a terminal event"
                ));
            };
            match event {
                RunEvent::AssistantTextDelta { text } => output.push_str(&text),
                RunEvent::Completed { .. } => break Ok(DesignImporterOutcome::Completed(output)),
                RunEvent::Cancelled { .. } => break Ok(DesignImporterOutcome::Cancelled),
                RunEvent::Failed { message, .. } => {
                    break Err(anyhow::anyhow!(message).context("design importer failed"));
                }
                _ => {}
            }
        };
        *self
            .cancellation
            .lock()
            .expect("design importer cancellation mutex poisoned") = None;
        result
    }
}

fn import_input(prompt: String, contact_sheet: Option<PathBuf>) -> Result<UserInput> {
    let Some(path) = contact_sheet else {
        return Ok(UserInput::text(prompt));
    };
    let bytes = std::fs::read(&path)
        .with_context(|| format!("reading design contact sheet {}", path.display()))?;
    if bytes.len() > MAX_CONTACT_SHEET_BYTES {
        bail!("design contact sheet exceeds the 32 MiB attachment limit");
    }
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(UserInput::text_and_images(
        prompt,
        [ImageContent {
            data,
            mime_type: "image/png".into(),
        }],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_input_does_not_require_an_attachment() {
        assert!(import_input("analyze".into(), None).is_ok());
    }

    #[test]
    fn missing_contact_sheet_is_actionable() {
        let error = import_input(
            "analyze".into(),
            Some(PathBuf::from("/missing/contact-sheet.png")),
        )
        .unwrap_err();
        assert!(error.to_string().contains("reading design contact sheet"));
    }
}

//! Account management in the caller's existing terminal session.

use anyhow::Result;
use crossterm::event::EventStream;
use rho_providers::{provider, CredentialStore};

use super::{
    auth_summary, authenticate_mode, choose_auth, discover_models, provider_has_auth_picker,
    select, CrosstermTerminal, Navigation, ProviderAuthKind, ProviderAuthentication,
    SlideCredentialStore,
};

/// Connect an account without changing the selected provider, auth mode, or model.
/// Reuses the caller's input stream throughout the flow.
pub async fn login(
    terminal: &mut CrosstermTerminal,
    input: &mut EventStream,
) -> Result<Option<String>> {
    let providers = provider::providers();
    let rows = providers
        .iter()
        .map(|provider| format!("{:<22} {}", provider.display_name, auth_summary(provider)))
        .collect::<Vec<_>>();
    'providers: loop {
        let index = match select(
            terminal,
            input,
            " Connect a provider ",
            &[
                "Connect a provider or replace its credentials.",
                "Your current model will not change.",
            ],
            &rows,
            0,
            "Enter continue  ·  ↑/↓ move  ·  Esc cancel",
        )
        .await?
        {
            Navigation::Selected(index) => index,
            Navigation::Back => return Ok(None),
        };
        let provider = &providers[index];
        let has_auth_picker = provider_has_auth_picker(provider.name)?;
        loop {
            let auth = match choose_auth(terminal, input, provider.name).await? {
                Navigation::Selected(auth) => auth,
                Navigation::Back => continue 'providers,
            };
            match authenticate_mode(
                terminal,
                input,
                provider.name,
                auth,
                /*reuse_existing*/ false,
                /*diagnostic*/ None,
            )
            .await?
            {
                Navigation::Back if has_auth_picker => continue,
                Navigation::Back => continue 'providers,
                Navigation::Selected(()) => {
                    let mut message = match auth.auth_kind {
                        ProviderAuthKind::None => {
                            format!("{} does not require sign-in.", provider.display_name)
                        }
                        ProviderAuthKind::OllamaDeviceKey { .. } => {
                            "Ollama Cloud device setup acknowledged. Approval is managed by Ollama; slide-builder cannot confirm completion.".to_owned()
                        }
                        ProviderAuthKind::ApiKey { .. }
                        | ProviderAuthKind::CodexOAuth { .. }
                        | ProviderAuthKind::GithubCopilotDevice { .. }
                        | ProviderAuthKind::KimiOAuth { .. }
                        | ProviderAuthKind::XaiOAuth { .. }
                        | ProviderAuthKind::BearerCredential { .. } => format!(
                            "Saved credentials for {} · {}.",
                            provider.display_name, auth.login_label
                        ),
                    };
                    if ProviderAuthentication::has_environment_override(auth.id) {
                        message.push_str(" An environment credential still takes precedence.");
                    }
                    if let Err(error) = discover_models(provider.name, auth.id).await {
                        message.push_str(&format!(" Model discovery failed: {error}"));
                    }
                    return Ok(Some(message));
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StoredConnection {
    auth_id: &'static str,
    label: String,
}

/// Rho considers local and external Ollama auth "stored" too. Exclude both:
/// neither represents a credential this application's keyring can remove.
fn stored_connections(store: &dyn CredentialStore) -> Result<Vec<StoredConnection>> {
    let mut connections = Vec::new();
    for provider in provider::providers() {
        for auth in provider.auth_modes() {
            match auth.auth_kind {
                ProviderAuthKind::None | ProviderAuthKind::OllamaDeviceKey { .. } => continue,
                ProviderAuthKind::ApiKey { .. }
                | ProviderAuthKind::CodexOAuth { .. }
                | ProviderAuthKind::GithubCopilotDevice { .. }
                | ProviderAuthKind::KimiOAuth { .. }
                | ProviderAuthKind::XaiOAuth { .. }
                | ProviderAuthKind::BearerCredential { .. } => {}
            }
            if ProviderAuthentication::has_stored_credentials(store, auth.id)? {
                connections.push(StoredConnection {
                    auth_id: auth.id,
                    label: format!("{} · {}", provider.display_name, auth.login_label),
                });
            }
        }
    }
    Ok(connections)
}

/// Remove one saved auth connection after confirmation, leaving config untouched.
/// Environment credentials and externally managed device keys are never removed.
pub async fn logout(
    terminal: &mut CrosstermTerminal,
    input: &mut EventStream,
) -> Result<Option<String>> {
    let connections = stored_connections(&SlideCredentialStore)?;
    if connections.is_empty() {
        return Ok(Some(
            "No saved connections to remove. Environment credentials and Ollama device sign-in are managed outside slide-builder.".to_owned(),
        ));
    }
    let rows = connections
        .iter()
        .map(|entry| entry.label.clone())
        .collect::<Vec<_>>();
    let mut selected = 0;
    loop {
        selected = match select(
            terminal,
            input,
            " Disconnect a provider ",
            &[
                "Remove a saved slide-builder connection.",
                "Environment and external credentials stay.",
            ],
            &rows,
            selected,
            "Enter continue  ·  ↑/↓ move  ·  Esc cancel",
        )
        .await?
        {
            Navigation::Selected(index) => index,
            Navigation::Back => return Ok(None),
        };
        let connection = &connections[selected];
        let confirmation = select(
            terminal,
            input,
            " Remove saved credentials? ",
            &[
                &connection.label,
                "This removes credentials, not your account.",
                "Environment credentials remain active.",
            ],
            &[
                "Keep connection".to_owned(),
                "Remove saved credentials".to_owned(),
            ],
            0,
            "Enter confirm  ·  ↑/↓ move  ·  Esc back",
        )
        .await?;
        match confirmation {
            Navigation::Selected(1) => {
                let removed = ProviderAuthentication::delete_credentials(
                    &SlideCredentialStore,
                    connection.auth_id,
                )?;
                let mut message = if removed {
                    format!("Removed saved credentials for {}.", connection.label)
                } else {
                    format!("No saved credentials remain for {}.", connection.label)
                };
                if ProviderAuthentication::has_environment_override(connection.auth_id) {
                    message.push_str(" An environment credential remains active.");
                }
                return Ok(Some(message));
            }
            Navigation::Selected(_) | Navigation::Back => {}
        }
    }
}

#[cfg(test)]
#[path = "account_tests.rs"]
mod tests;

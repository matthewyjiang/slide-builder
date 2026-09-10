//! Workspace entry points for provider connection management.
use std::io;

use anyhow::{bail, Result};
use crossterm::event::EventStream;
use ratatui::{backend::CrosstermBackend, Terminal};
use rho_providers::auth::login_dispatch::ProviderAuthentication;
use slide_builder::{config::Config, credentials::SlideCredentialStore};

pub enum ConnectionAction {
    Login,
    Logout,
}

pub fn ensure_connected(config: &Config) -> Result<()> {
    if !ProviderAuthentication::has_credentials(&SlideCredentialStore, config.auth_mode()?)? {
        bail!("The current provider is disconnected. Use /login to reconnect or /model to choose another provider.");
    }
    Ok(())
}

/// Account flows share the workspace's input stream.
pub async fn manage(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    input: &mut EventStream,
    action: ConnectionAction,
) -> Result<Option<String>> {
    match action {
        ConnectionAction::Login => crate::onboarding::login(terminal, input).await,
        ConnectionAction::Logout => crate::onboarding::logout(terminal, input).await,
    }
}

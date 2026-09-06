//! Models slide-builder can actually run: rho's catalog filtered to the
//! providers whose credentials are in slide-builder's keyring.
//!
//! Both the `/model` picker and the configuration menu consume this list so
//! neither surface can offer a provider the user has not logged in to.
use rho_providers::{
    credentials::available_auth_modes,
    model::catalog::{available_models_for_auths, SelectionAuthContext},
};

use crate::{config::Config, credentials::SlideCredentialStore};

/// One selectable model together with the auth mode it will run under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableModel {
    pub provider: String,
    pub model: String,
    pub display_name: String,
    pub auth: String,
}

impl AvailableModel {
    /// `provider/model`, the reference shown in the status line and used as
    /// the stable choice value in menus.
    pub fn reference(&self) -> String {
        rho_providers::provider::model_reference(&self.provider, &self.model)
    }

    /// Entry for whatever the config currently points at, used when the
    /// configured model is no longer in the discovered list.
    pub fn from_config(config: &Config) -> Self {
        Self {
            provider: config.provider.clone(),
            model: config.model.clone(),
            display_name: config.model.clone(),
            auth: config.auth_mode().map(str::to_owned).unwrap_or_default(),
        }
    }

    pub fn matches_config(&self, config: &Config) -> bool {
        self.provider == config.provider && self.model == config.model
    }
}

/// Reads the keyring once and returns every model rho knows how to run with
/// the stored credentials, sorted by provider then model.
pub fn discover_available_models(config: &Config) -> Vec<AvailableModel> {
    let auths = available_auth_modes(&SlideCredentialStore);
    let context = SelectionAuthContext {
        current: config.auth_mode().ok(),
        available: &auths,
    };
    available_models_for_auths(&auths)
        .into_iter()
        .map(|entry| AvailableModel {
            auth: context.select(&entry.auth_modes),
            provider: entry.provider,
            model: entry.model,
            display_name: entry.display_name,
        })
        .collect()
}

//! Slide-builder-owned provider credentials.
//!
//! Account names follow rho-provider conventions, but secrets live under a
//! distinct OS-keyring service so installing slide-builder never reads or
//! modifies credentials belonging to the `rho` application.
use rho_providers::{CredentialError, CredentialResult, CredentialStore};

pub const KEYRING_SERVICE: &str = "slide-builder";

#[derive(Clone, Debug, Default)]
pub struct SlideCredentialStore;

impl SlideCredentialStore {
    fn entry(account: &str) -> CredentialResult<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, account)
            .map_err(|error| CredentialError::StoreUnavailable(error.to_string()))
    }
}

impl CredentialStore for SlideCredentialStore {
    fn get_secret(&self, account: &str) -> CredentialResult<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(CredentialError::StoreUnavailable(error.to_string())),
        }
    }

    fn set_secret(&self, account: &str, secret: &str) -> CredentialResult<()> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|error| CredentialError::StoreUnavailable(error.to_string()))
    }

    fn delete_secret(&self, account: &str) -> CredentialResult<bool> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(CredentialError::StoreUnavailable(error.to_string())),
        }
    }
}

/// Save an API key using provider-specific account naming, but in the isolated
/// slide-builder keyring service.
pub fn save_api_key(provider: &str, key: &str) -> anyhow::Result<()> {
    if key.trim().is_empty() {
        anyhow::bail!("API key cannot be empty");
    }
    rho_providers::credentials::save_provider_api_key(&SlideCredentialStore, provider, key)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_is_not_rho() {
        assert_ne!(KEYRING_SERVICE, "rho");
    }
}

use super::*;
use rho_providers::credentials::MemoryCredentialStore;

fn connection(auth_id: &'static str) -> StoredConnection {
    let (provider, auth) = provider::resolve_auth_mode(auth_id).unwrap();
    StoredConnection {
        auth_id,
        label: format!("{} · {}", provider.display_name, auth.login_label),
    }
}

#[test]
fn empty_store_has_no_removable_connections() {
    // Independent of environment keys and any externally installed Ollama key.
    assert_eq!(
        stored_connections(&MemoryCredentialStore::default()).unwrap(),
        Vec::<StoredConnection>::new()
    );
}

#[test]
fn listing_includes_only_nonempty_stored_auth_modes() {
    let store = MemoryCredentialStore::default();
    ProviderAuthentication::save_api_key(&store, "xai-api-key", "test-key").unwrap();
    let (_, oauth) = provider::resolve_auth_mode("openrouter-oauth").unwrap();
    store
        .set_secret(oauth.auth_kind.account().unwrap(), "test-oauth-key")
        .unwrap();
    let (_, blank) = provider::resolve_auth_mode("openrouter-api-key").unwrap();
    store
        .set_secret(blank.auth_kind.account().unwrap(), "  ")
        .unwrap();
    assert_eq!(
        stored_connections(&store).unwrap(),
        vec![connection("openrouter-oauth"), connection("xai-api-key")]
    );
}

#[test]
fn removing_one_auth_mode_preserves_other_modes_and_providers() {
    let store = MemoryCredentialStore::default();
    ProviderAuthentication::save_api_key(&store, "openrouter-api-key", "api-key").unwrap();
    ProviderAuthentication::save_api_key(&store, "xai-api-key", "other-key").unwrap();
    let (_, oauth) = provider::resolve_auth_mode("openrouter-oauth").unwrap();
    store
        .set_secret(oauth.auth_kind.account().unwrap(), "oauth-key")
        .unwrap();

    assert!(ProviderAuthentication::delete_credentials(&store, "openrouter-api-key").unwrap());
    assert!(!ProviderAuthentication::delete_credentials(&store, "openrouter-api-key").unwrap());
    assert_eq!(
        stored_connections(&store).unwrap(),
        vec![connection("openrouter-oauth"), connection("xai-api-key")]
    );
    assert_eq!(
        store
            .get_secret(oauth.auth_kind.account().unwrap())
            .unwrap(),
        Some("oauth-key".to_owned())
    );
}

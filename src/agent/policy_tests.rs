use super::*;
use rho_sdk::{CapabilitySource, NetworkTarget};

#[test]
fn network_access_is_allowed_in_all_permission_modes() {
    for mode in [
        PermissionMode::Auto,
        PermissionMode::Supervised,
        PermissionMode::Plan,
    ] {
        let policy = SlidePolicy::new(mode, "/decks", "/cache");
        for target in [
            NetworkTarget::Url("https://example.com".into()),
            NetworkTarget::ToolManaged,
        ] {
            let request = CapabilityRequest::network(
                target,
                CapabilitySource::HostProvidedTool {
                    name: "fetch_content".into(),
                },
            );
            assert_eq!(policy.evaluate(&request), PolicyDecision::Allow, "{mode:?}");
        }
    }
}

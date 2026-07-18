use rho_sdk::{CapabilityOperation, CapabilityRequest, PolicyDecision, WorkspacePolicy};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    Auto,
    #[default]
    Supervised,
    Plan,
}

#[derive(Debug, Clone)]
pub struct SlidePolicy {
    mode: PermissionMode,
    decks: PathBuf,
    cache: PathBuf,
}
impl SlidePolicy {
    pub fn new(mode: PermissionMode, decks: impl Into<PathBuf>, cache: impl Into<PathBuf>) -> Self {
        Self {
            mode,
            decks: decks.into(),
            cache: cache.into(),
        }
    }
    fn product_path(&self, p: &Path) -> bool {
        p.starts_with(&self.decks) || p.starts_with(&self.cache)
    }
}
impl WorkspacePolicy for SlidePolicy {
    fn evaluate(&self, request: &CapabilityRequest) -> PolicyDecision {
        use CapabilityOperation::*;
        match request.operation() {
            ReadPath { .. } | LoadSkill { .. } | DiscoverInstructions { .. } => {
                PolicyDecision::Allow
            }
            NetworkAccess(_) => PolicyDecision::Deny {
                reason: "agent-tool network access is disabled in v1".into(),
            },
            WritePath { path, .. } if self.mode == PermissionMode::Plan => PolicyDecision::Deny {
                reason: "writes are disabled in plan mode".into(),
            },
            ExecuteProcess(_) if self.mode == PermissionMode::Plan => PolicyDecision::Deny {
                reason: "processes are disabled in plan mode".into(),
            },
            WritePath { path, .. } if self.product_path(path) => PolicyDecision::Allow,
            WritePath { .. } | ExecuteProcess(_) if self.mode == PermissionMode::Auto => {
                PolicyDecision::Allow
            }
            WritePath { .. } => PolicyDecision::RequireApproval {
                reason: "writing outside the deck/cache requires approval".into(),
            },
            ExecuteProcess(_) => PolicyDecision::RequireApproval {
                reason: "agent-requested processes require approval".into(),
            },
            _ => PolicyDecision::Deny {
                reason: "unsupported capability".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rho_sdk::{CapabilitySource, PathScope};
    fn req(p: &str) -> CapabilityRequest {
        CapabilityRequest::write_path(
            p,
            PathScope::PrimaryWorkspace,
            CapabilitySource::HostProvidedTool {
                name: "test".into(),
            },
        )
    }
    #[test]
    fn decision_table() {
        let p = SlidePolicy::new(PermissionMode::Supervised, "/decks", "/cache");
        assert_eq!(p.evaluate(&req("/decks/a.pptx")), PolicyDecision::Allow);
        assert!(matches!(
            p.evaluate(&req("/repo/a")),
            PolicyDecision::RequireApproval { .. }
        ));
        let q = SlidePolicy::new(PermissionMode::Plan, "/decks", "/cache");
        assert!(matches!(
            q.evaluate(&req("/decks/a.pptx")),
            PolicyDecision::Deny { .. }
        ));
    }
}

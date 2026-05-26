//! Permission mode shared by tool and privacy policies.

use serde::{Deserialize, Serialize};

/// How an action is mediated: allowed outright, gated on approval, or blocked.
///
/// Used by both tool permissions and privacy (remote/local) policies. Variants
/// serialize to their lowercase name. The ordering `Allow < Ask < Deny` reflects
/// increasing strictness, so the stricter of two modes is the greater one — the
/// layered merge relies on this to ensure a higher layer can only tighten, never
/// weaken, a mode.
///
/// # Examples
///
/// ```
/// use smista_core::policy::PermissionMode;
///
/// assert!(PermissionMode::Deny > PermissionMode::Ask);
/// assert_eq!(PermissionMode::Ask.max(PermissionMode::Allow), PermissionMode::Ask);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// The action runs without confirmation.
    Allow,
    /// The user is asked to approve the action.
    #[default]
    Ask,
    /// The action is blocked.
    Deny,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_ask() {
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);
    }

    #[test]
    fn should_order_by_increasing_strictness() {
        assert!(PermissionMode::Allow < PermissionMode::Ask);
        assert!(PermissionMode::Ask < PermissionMode::Deny);
    }

    #[test]
    fn should_serialize_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&PermissionMode::Deny).unwrap(),
            "\"deny\""
        );
    }

    #[test]
    fn should_roundtrip_every_variant() {
        for mode in [
            PermissionMode::Allow,
            PermissionMode::Ask,
            PermissionMode::Deny,
        ] {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(serde_json::from_str::<PermissionMode>(&json).unwrap(), mode);
        }
    }
}

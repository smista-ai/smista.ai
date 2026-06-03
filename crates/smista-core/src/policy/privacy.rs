//! Privacy constraints applied when selecting and forwarding context.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::PermissionMode;
use super::glob::compile_globs;

/// Privacy policy controlling what context may reach which model classes.
///
/// Mirrors the `[privacy]` configuration: top-level `restricted_paths`, plus a
/// [`remote`](Self::remote) and [`local`](Self::local) sub-policy. These are
/// **safety-critical**: a higher *preference* layer must never silently weaken
/// them, so the CLI merge unions path lists and only lets preference layers
/// tighten the modes (see `smista-cli`'s `config::layers`).
///
/// Paths are glob patterns kept verbatim; the matcher owns interpretation.
///
/// # Examples
///
/// ```
/// use smista_core::policy::{PermissionMode, PrivacyPolicy};
///
/// let policy: PrivacyPolicy = serde_json::from_value(serde_json::json!({
///     "restricted_paths": [".env", "secrets/**"],
///     "remote": { "mode": "ask", "blocked_paths": [".env"] },
///     "local": { "mode": "allow" },
/// }))
/// .unwrap();
/// assert_eq!(policy.remote.mode(), PermissionMode::Ask);
/// assert_eq!(policy.local.mode(), PermissionMode::Allow);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    /// Paths treated as sensitive across all model classes.
    #[serde(default)]
    pub restricted_paths: Vec<String>,
    /// Constraints applied when sending context to remote providers.
    #[serde(default)]
    pub remote: RemotePrivacy,
    /// Constraints applied when sending context to local models.
    #[serde(default)]
    pub local: LocalPrivacy,
}

impl PrivacyPolicy {
    /// Returns `true` when `path` must not be sent to remote providers.
    ///
    /// Combines [`restricted_paths`](Self::restricted_paths) and
    /// [`remote.blocked_paths`](RemotePrivacy::blocked_paths). Globs are
    /// matched via `globset`. Safety-critical: if a glob fails to compile, the
    /// path is treated as restricted.
    #[must_use]
    pub fn is_restricted_for_remote(&self, path: &Path) -> bool {
        let mut patterns = self.restricted_paths.clone();
        patterns.extend(self.remote.blocked_paths.iter().cloned());
        let restricted = match compile_globs(&patterns) {
            Ok(set) => set.is_match(path),
            Err(source) => {
                tracing::warn!(
                    privacy.path = %path.display(),
                    error.message = %source,
                    "invalid privacy glob; failing closed and restricting {{privacy.path}}"
                );
                true
            }
        };
        tracing::debug!(
            privacy.path = %path.display(),
            privacy.restricted = restricted,
            "remote restriction check for {{privacy.path}}: restricted={{privacy.restricted}}"
        );
        restricted
    }
}

/// Privacy constraints for remote providers.
///
/// The effective [`mode`](Self::mode) defaults to [`PermissionMode::Ask`] when
/// unset: disclosing context to a remote provider requires approval unless
/// explicitly allowed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RemotePrivacy {
    /// How sending context to remote providers is mediated, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    /// Paths that must never be sent to remote providers.
    #[serde(default)]
    pub blocked_paths: Vec<String>,
}

impl RemotePrivacy {
    /// Returns the effective mode, defaulting to [`PermissionMode::Ask`].
    #[must_use]
    pub fn mode(&self) -> PermissionMode {
        self.mode.unwrap_or(PermissionMode::Ask)
    }
}

/// Privacy constraints for local models.
///
/// The effective [`mode`](Self::mode) defaults to [`PermissionMode::Allow`] when
/// unset: local models may receive context without prompting.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LocalPrivacy {
    /// How sending context to local models is mediated, if set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
}

impl LocalPrivacy {
    /// Returns the effective mode, defaulting to [`PermissionMode::Allow`].
    #[must_use]
    pub fn mode(&self) -> PermissionMode {
        self.mode.unwrap_or(PermissionMode::Allow)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn should_default_to_empty_paths_and_safe_modes() {
        let policy = PrivacyPolicy::default();
        assert!(policy.restricted_paths.is_empty());
        assert!(policy.remote.blocked_paths.is_empty());
        assert_eq!(policy.remote.mode(), PermissionMode::Ask);
        assert_eq!(policy.local.mode(), PermissionMode::Allow);
    }

    #[test]
    fn should_deserialize_from_empty_table() {
        let policy: PrivacyPolicy = serde_json::from_str("{}").unwrap();
        assert_eq!(policy, PrivacyPolicy::default());
    }

    #[test]
    fn should_apply_effective_mode_defaults_when_unset() {
        let policy = PrivacyPolicy::default();
        assert!(policy.remote.mode.is_none());
        assert_eq!(policy.remote.mode(), PermissionMode::Ask);
        assert_eq!(policy.local.mode(), PermissionMode::Allow);
    }

    #[test]
    fn should_roundtrip_serde() {
        let policy: PrivacyPolicy = serde_json::from_value(serde_json::json!({
            "restricted_paths": [".env", "secrets/**", "*.pem"],
            "remote": { "mode": "deny", "blocked_paths": [".env"] },
            "local": { "mode": "allow" },
        }))
        .unwrap();
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(
            serde_json::from_str::<PrivacyPolicy>(&json).unwrap(),
            policy
        );
    }

    #[test]
    fn should_fail_closed_on_invalid_glob() {
        let mut policy = PrivacyPolicy::default();
        policy.restricted_paths.push("[".to_string());
        assert!(policy.is_restricted_for_remote(Path::new("anything")));
    }

    #[test]
    fn should_restrict_paths_for_remote() {
        let policy: PrivacyPolicy = serde_json::from_value(serde_json::json!({
            "restricted_paths": ["secrets/**", "*.pem"],
            "remote": { "mode": "ask", "blocked_paths": [".env"] },
            "local": { "mode": "allow" },
        }))
        .unwrap();

        assert!(policy.is_restricted_for_remote(Path::new("secrets/db.txt")));
        assert!(policy.is_restricted_for_remote(Path::new("key.pem")));
        assert!(policy.is_restricted_for_remote(Path::new(".env")));
        assert!(!policy.is_restricted_for_remote(Path::new("src/main.rs")));
    }
}

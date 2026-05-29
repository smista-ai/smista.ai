//! Tool permissions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::PermissionMode;
use crate::error::PolicyError;

/// Tool permissions: the mode required for each named tool.
///
/// Mirrors the `[tools.permissions]` table, a flat map of tool name to
/// [`PermissionMode`] (e.g. `file_write = "ask"`). A tool with no entry has no
/// explicitly configured mode; callers decide the safe fallback.
///
/// # Examples
///
/// ```
/// use smista_core::policy::{PermissionMode, ToolsConfig};
///
/// let config: ToolsConfig = serde_json::from_value(serde_json::json!({
///     "permissions": { "file_read": "allow", "shell": "ask", "network": "deny" }
/// }))
/// .unwrap();
/// assert_eq!(config.mode_for("file_read"), Some(PermissionMode::Allow));
/// assert_eq!(config.mode_for("network"), Some(PermissionMode::Deny));
/// assert_eq!(config.mode_for("unlisted"), None);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Per-tool permission mode, keyed by tool name.
    #[serde(default)]
    pub permissions: BTreeMap<String, PermissionMode>,
}

impl ToolsConfig {
    /// Returns the configured mode for `tool`, or `None` if none is set.
    #[must_use]
    pub fn mode_for(&self, tool: &str) -> Option<PermissionMode> {
        self.permissions.get(tool).copied()
    }

    /// Sets the mode for `tool`.
    pub fn set(&mut self, tool: impl Into<String>, mode: PermissionMode) {
        self.permissions.insert(tool.into(), mode);
    }

    /// Merges `over` onto `self`, allowing an override to tighten a tool's mode
    /// (`allow -> ask -> deny`) but never loosen it.
    ///
    /// > TL;DR a `deny` can't be overridden to `ask` or `allow`, but an `allow` can be tightened to `ask` or `deny`.
    ///
    /// A new tool absent from `self` is accepted as-is. A loosening attempt
    /// returns [`PolicyError::PermissionExpansion`] naming the tool.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::PermissionExpansion`] when `over` weakens a mode.
    pub fn narrow(mut self, over: &ToolsConfig) -> Result<Self, PolicyError> {
        tracing::debug!(
            tools.base = self.permissions.len(),
            tools.over = over.permissions.len(),
            "narrowing {{tools.base}} base permissions with {{tools.over}} overrides"
        );
        for (tool, &requested) in &over.permissions {
            if let Some(&base) = self.permissions.get(tool)
                && requested < base
            {
                tracing::warn!(
                    tool.name = %tool,
                    tool.base = %base,
                    tool.requested = %requested,
                    "override loosens permission for {{tool.name}}; rejecting"
                );
                return Err(PolicyError::PermissionExpansion {
                    base,
                    requested,
                    tool: tool.clone(),
                });
            }
            self.permissions.insert(tool.clone(), requested);
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PolicyError;

    #[test]
    fn should_default_to_empty_permissions() {
        assert!(ToolsConfig::default().permissions.is_empty());
    }

    #[test]
    fn should_return_none_for_unlisted_tool() {
        assert_eq!(ToolsConfig::default().mode_for("anything"), None);
    }

    #[test]
    fn should_error_when_override_loosens() {
        let mut base = ToolsConfig::default();
        base.set("shell", PermissionMode::Deny);

        let mut over = ToolsConfig::default();
        over.set("shell", PermissionMode::Allow);

        let err = base.narrow(&over).unwrap_err();
        assert_eq!(
            err,
            PolicyError::PermissionExpansion {
                base: PermissionMode::Deny,
                requested: PermissionMode::Allow,
                tool: "shell".to_string(),
            }
        );
    }

    #[test]
    fn should_return_configured_mode() {
        let mut config = ToolsConfig::default();
        config.set("shell", PermissionMode::Deny);
        assert_eq!(config.mode_for("shell"), Some(PermissionMode::Deny));
    }

    #[test]
    fn should_parse_flat_permissions_table() {
        let config: ToolsConfig = serde_json::from_value(serde_json::json!({
            "permissions": {
                "file_read": "allow",
                "file_write": "ask",
                "network": "deny",
            }
        }))
        .unwrap();
        assert_eq!(config.mode_for("file_write"), Some(PermissionMode::Ask));
    }

    #[test]
    fn should_roundtrip_serde() {
        let mut config = ToolsConfig::default();
        config.set("git", PermissionMode::Allow);
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<ToolsConfig>(&json).unwrap(), config);
    }

    #[test]
    fn should_tighten_permissions_on_narrow() {
        let mut base = ToolsConfig::default();
        base.set("shell", PermissionMode::Ask);
        base.set("network", PermissionMode::Allow);

        let mut over = ToolsConfig::default();
        over.set("shell", PermissionMode::Deny);
        over.set("git", PermissionMode::Deny);

        let merged = base.narrow(&over).unwrap();
        assert_eq!(merged.mode_for("shell"), Some(PermissionMode::Deny));
        assert_eq!(merged.mode_for("network"), Some(PermissionMode::Allow));
        assert_eq!(merged.mode_for("git"), Some(PermissionMode::Deny));
    }
}

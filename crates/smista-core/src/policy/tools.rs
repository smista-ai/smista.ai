//! Tool permissions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::PermissionMode;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_empty_permissions() {
        assert!(ToolsConfig::default().permissions.is_empty());
    }

    #[test]
    fn should_return_none_for_unlisted_tool() {
        assert_eq!(ToolsConfig::default().mode_for("anything"), None);
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
}

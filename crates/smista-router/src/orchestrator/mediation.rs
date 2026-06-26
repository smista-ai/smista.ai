//! Tool mediation: partitioning the model's tool calls by permission.
//!
//! The model may ask to run any offered tool; the deterministic tool policy
//! decides what actually happens. A `deny` (or, while planning, any
//! file-changing tool) is refused with a reason fed back to the model; an
//! `allow` runs without confirmation; an `ask` (or an unset mode) runs only
//! after the user approves. Read-only tools are never denied by plan mode.
#![allow(
    dead_code,
    reason = "wired into the turn loop in a later orchestrator task"
)]

use smista_core::api::ToolApproval;
use smista_core::policy::{PermissionMode, ToolsConfig};
use smista_providers::api::ToolCall;

use crate::orchestrator::tools::tool_changes_files;

/// The outcome of mediating one turn's tool calls.
pub(crate) struct Mediated {
    /// Calls refused before reaching the client, each with a reason to feed
    /// back to the model.
    pub(crate) denied: Vec<(ToolCall, &'static str)>,
    /// Calls the client must run, with their approval requirement.
    pub(crate) client: Vec<ClientCall>,
}

/// A tool call to hand to the client, with whether it needs approval first.
pub(crate) struct ClientCall {
    /// The model's tool call.
    pub(crate) call: ToolCall,
    /// Whether the user must approve before the call runs.
    pub(crate) requires_approval: ToolApproval,
}

/// Partitions `calls` into denied and client-bound by the tool `policy`.
///
/// In `plan_mode`, any file-changing tool is denied regardless of its policy
/// mode; read-only tools are unaffected. Otherwise a `deny` is refused, an
/// `allow` runs without confirmation, and an `ask` (or unset mode) runs only
/// after approval.
pub(crate) fn mediate(calls: Vec<ToolCall>, policy: &ToolsConfig, plan_mode: bool) -> Mediated {
    let mut denied = Vec::new();
    let mut client = Vec::new();
    for call in calls {
        if plan_mode && tool_changes_files(&call.name) {
            tracing::debug!(tool = %call.name, "denying file-changing tool while planning");
            denied.push((call, "denied: planning mode forbids changing the machine"));
            continue;
        }
        match policy.mode_for(&call.name).unwrap_or(PermissionMode::Ask) {
            PermissionMode::Deny => {
                tracing::debug!(tool = %call.name, "denying tool by policy");
                denied.push((call, "denied by policy"));
            }
            PermissionMode::Allow => client.push(ClientCall {
                call,
                requires_approval: ToolApproval::Allow,
            }),
            PermissionMode::Ask => client.push(ClientCall {
                call,
                requires_approval: ToolApproval::Ask,
            }),
        }
    }
    Mediated { denied, client }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn call(name: &str) -> ToolCall {
        ToolCall {
            call_id: format!("{name}-1"),
            name: name.to_string(),
            arguments: json!({}),
        }
    }

    #[test]
    fn should_partition_by_permission() {
        let mut config = ToolsConfig::default();
        config.set("read_file", PermissionMode::Allow);
        config.set("shell", PermissionMode::Deny);
        // edit_file unset -> Ask
        let calls = vec![call("read_file"), call("shell"), call("edit_file")];
        let mediated = mediate(calls, &config, false);

        assert_eq!(mediated.denied.len(), 1);
        assert!(
            mediated
                .client
                .iter()
                .any(|client| client.call.name == "read_file"
                    && client.requires_approval == ToolApproval::Allow)
        );
        assert!(
            mediated
                .client
                .iter()
                .any(|client| client.call.name == "edit_file"
                    && client.requires_approval == ToolApproval::Ask)
        );
    }

    #[test]
    fn should_deny_file_changing_tools_in_plan_mode() {
        let config = ToolsConfig::default();
        let mediated = mediate(vec![call("write_file"), call("read_file")], &config, true);
        assert!(mediated.denied.iter().any(|(c, _)| c.name == "write_file"));
        assert!(
            mediated
                .client
                .iter()
                .any(|client| client.call.name == "read_file")
        );
    }
}

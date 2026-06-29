//! The built-in tool catalog the router offers to the model.
//!
//! `ExecuteRequest` carries only `tools.permissions` (name → mode) and the
//! invoked/available skills; the provider needs full [`ToolDefinition`]s. This
//! module ships the built-in catalog (`read_file`, `write_file`, `edit_file`,
//! `shell`, plus the memory tool), gates it by the policy's permission modes,
//! and offers one tool per skill alongside.
use serde_json::{Value, json};
use smista_core::policy::{PermissionMode, ToolsConfig};
use smista_core::skill::Skill;
use smista_providers::api::ToolDefinition;

/// The name of the built-in tool that reads a file on the user's machine.
pub(crate) const READ_FILE_TOOL: &str = "read_file";
/// The name of the built-in tool that writes a file on the user's machine.
pub(crate) const WRITE_FILE_TOOL: &str = "write_file";
/// The name of the built-in tool that edits a file on the user's machine.
pub(crate) const EDIT_FILE_TOOL: &str = "edit_file";
/// The name of the built-in tool that runs a shell command on the user's machine.
pub(crate) const SHELL_TOOL: &str = "shell";
/// The name of the agent-internal memory tool, offered alongside the catalog.
pub(crate) const MEMORY_TOOL: &str = "memory";

/// A built-in tool the router can offer the model.
pub(crate) struct ToolSpec {
    /// The tool's unique name, as referenced by a tool call.
    pub(crate) name: &'static str,
    /// A human-readable description the model uses to decide when to call it.
    pub(crate) description: &'static str,
    /// Builds the JSON-Schema describing the tool's arguments.
    pub(crate) parameters: fn() -> Value,
    /// Whether invoking the tool changes files on the user's machine.
    pub(crate) changes_files: bool,
}

fn read_file_parameters() -> Value {
    json!({
        "type": "object",
        "properties": { "path": { "type": "string", "description": "Path of the file to read." } },
        "required": ["path"]
    })
}

fn write_file_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path of the file to write." },
            "content": { "type": "string", "description": "Full new file content." }
        },
        "required": ["path", "content"]
    })
}

fn edit_file_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Path of the file to edit." },
            "old": { "type": "string", "description": "Exact text to replace." },
            "new": { "type": "string", "description": "Replacement text." }
        },
        "required": ["path", "old", "new"]
    })
}

fn shell_parameters() -> Value {
    json!({
        "type": "object",
        "properties": { "command": { "type": "string", "description": "Shell command to run." } },
        "required": ["command"]
    })
}

fn memory_parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": { "type": "string", "description": "Memory operation to perform." },
            "key": { "type": "string", "description": "Memory key." },
            "value": { "type": "string", "description": "Memory value." }
        },
        "required": ["operation"]
    })
}

/// The built-in tools, before policy gating and skills are applied.
pub(crate) fn builtin_catalog() -> &'static [ToolSpec] {
    &[
        ToolSpec {
            name: READ_FILE_TOOL,
            description: "Read the contents of a file on the user's machine.",
            parameters: read_file_parameters,
            changes_files: false,
        },
        ToolSpec {
            name: WRITE_FILE_TOOL,
            description: "Write (creating or overwriting) a file on the user's machine.",
            parameters: write_file_parameters,
            changes_files: true,
        },
        ToolSpec {
            name: EDIT_FILE_TOOL,
            description: "Replace a span of text in a file on the user's machine.",
            parameters: edit_file_parameters,
            changes_files: true,
        },
        ToolSpec {
            name: SHELL_TOOL,
            description: "Run a shell command on the user's machine.",
            parameters: shell_parameters,
            changes_files: false,
        },
    ]
}

/// Whether a tool changes files on the user's machine.
///
/// Used by plan mode to deny file-changing tools while only a plan is being
/// produced. A tool absent from the catalog is treated as non-file-changing.
pub(crate) fn tool_changes_files(name: &str) -> bool {
    builtin_catalog()
        .iter()
        .find(|spec| spec.name == name)
        .is_some_and(|spec| spec.changes_files)
}

/// Builds the diff a file-changing tool call records, as `(path, body)`.
///
/// Returns `None` for any call that does not change files, so the run loop
/// records a diff exactly for `write_file` and `edit_file`. The body is a small
/// unified-diff rendering of the change the call requests, derived from the
/// model's arguments — the only point at which they are available — so it can be
/// stored (and, for an encrypted session, sealed) up front and marked applied
/// once the client confirms the edit.
pub(crate) fn diff_for_tool_call(name: &str, arguments: &Value) -> Option<(String, String)> {
    let path = arguments.get("path")?.as_str()?.to_string();
    let body = match name {
        WRITE_FILE_TOOL => {
            let content = arguments
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("");
            format!("--- /dev/null\n+++ {path}\n+{content}")
        }
        EDIT_FILE_TOOL => {
            let old = arguments.get("old").and_then(Value::as_str).unwrap_or("");
            let new = arguments.get("new").and_then(Value::as_str).unwrap_or("");
            format!("--- {path}\n+++ {path}\n-{old}\n+{new}")
        }
        _ => return None,
    };
    Some((path, body))
}

/// The full set of [`ToolDefinition`]s offered to the model for one turn.
///
/// Every built-in whose policy mode is not [`PermissionMode::Deny`] is offered
/// (an unset mode defaults to offered), plus the memory tool, plus one tool per
/// invoked and available skill.
pub(crate) fn offered_tools(
    policy: &ToolsConfig,
    invoked: &[Skill],
    available: &[Skill],
) -> Vec<ToolDefinition> {
    let mut tools: Vec<ToolDefinition> = builtin_catalog()
        .iter()
        .filter(|spec| policy.mode_for(spec.name) != Some(PermissionMode::Deny))
        .map(|spec| ToolDefinition {
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            parameters: (spec.parameters)(),
        })
        .collect();

    if policy.mode_for(MEMORY_TOOL) != Some(PermissionMode::Deny) {
        tools.push(ToolDefinition {
            name: MEMORY_TOOL.to_string(),
            description: "Record and recall durable memory across the session.".to_string(),
            parameters: memory_parameters(),
        });
    }

    tools.extend(
        invoked
            .iter()
            .chain(available.iter())
            .map(skill_tool_definition),
    );
    tools
}

/// Builds the [`ToolDefinition`] offered for a skill.
///
/// The model activates a skill by calling it; the description is the skill's
/// first line so the model can decide when it applies.
fn skill_tool_definition(skill: &Skill) -> ToolDefinition {
    ToolDefinition {
        name: skill.name.clone(),
        description: skill.content.lines().next().unwrap_or_default().to_string(),
        parameters: json!({ "type": "object", "properties": {} }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_offer_all_builtins_when_unconfigured() {
        let tools = offered_tools(&ToolsConfig::default(), &[], &[]);
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"shell"));
        assert!(names.contains(&"memory"));
    }

    #[test]
    fn should_not_offer_denied_tool() {
        let mut config = ToolsConfig::default();
        config.set("shell", PermissionMode::Deny);
        let tools = offered_tools(&config, &[], &[]);
        assert!(!tools.iter().any(|tool| tool.name == "shell"));
    }

    #[test]
    fn should_build_a_diff_for_an_edit_file_call() {
        let args = json!({ "path": "src/lib.rs", "old": "a", "new": "b" });
        let (path, body) = diff_for_tool_call("edit_file", &args).expect("edit produces a diff");
        assert_eq!(path, "src/lib.rs");
        assert!(body.contains("-a"));
        assert!(body.contains("+b"));
    }

    #[test]
    fn should_build_a_diff_for_a_write_file_call() {
        let args = json!({ "path": "new.rs", "content": "hello" });
        let (path, body) = diff_for_tool_call("write_file", &args).expect("write produces a diff");
        assert_eq!(path, "new.rs");
        assert!(body.contains("hello"));
    }

    #[test]
    fn should_not_build_a_diff_for_a_non_file_tool() {
        assert!(diff_for_tool_call("shell", &json!({ "command": "ls" })).is_none());
        assert!(diff_for_tool_call("read_file", &json!({ "path": "x" })).is_none());
    }

    #[test]
    fn should_flag_file_changing_tools() {
        assert!(tool_changes_files("write_file"));
        assert!(tool_changes_files("edit_file"));
        assert!(!tool_changes_files("read_file"));
        assert!(!tool_changes_files("shell"));
    }

    #[test]
    fn should_offer_one_tool_per_skill() {
        let invoked = [Skill {
            name: "code-review".to_string(),
            content: "Report findings by severity.\nMore detail.".to_string(),
        }];
        let tools = offered_tools(&ToolsConfig::default(), &invoked, &[]);
        let skill_tool = tools
            .iter()
            .find(|tool| tool.name == "code-review")
            .expect("skill tool not offered");
        assert_eq!(skill_tool.description, "Report findings by severity.");
    }
}

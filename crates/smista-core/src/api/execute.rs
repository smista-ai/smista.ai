//! Request and response bodies for running a task in a session.
//!
//! [`ExecuteRequest`] is the full input to `POST /sessions/{id}/execute` (and,
//! unchanged, to `/stream` and `/preview`): the user's input, the workspace
//! snapshot, the deterministic routing/permission/privacy policy, local
//! preferences, the available providers and credentials, and the assembled
//! context. [`ExecuteResponse`] reports the outcome: the routed model, what
//! context was used, the assistant message, usage, and the trace id.
//!
//! The `policy` block is the API wire snapshot the CLI sends, not the CLI's own
//! config schema: it carries a flat `permissions` map and a `default_model`
//! plus per-rule `match`/`use`, reusing
//! [`ModelReference`](crate::model::ModelReference),
//! [`TaskIntent`](crate::intent::TaskIntent) and
//! [`PermissionMode`](crate::policy::PermissionMode) for the leaf values.
//!
//! Provider credentials are never part of this body; they travel as request
//! headers and are handled at the HTTP boundary.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::TaskInput;
//! use smista_core::intent::TaskIntent;
//!
//! let input = TaskInput {
//!     text: "refactor the auth middleware".to_string(),
//!     command: Some(TaskIntent::Edit),
//!     explicit_model: None,
//! };
//! let json = serde_json::to_string(&input).unwrap();
//! assert!(json.contains("\"command\":\"edit\""));
//! ```

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::intent::TaskIntent;
use crate::message::Message;
use crate::model::{ModelReference, Provider};
use crate::policy::PermissionMode;
use crate::skill::Skill;
use crate::usage::Usage;

/// Full input to executing, streaming, or previewing a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecuteRequest {
    /// What the user is asking for.
    pub input: TaskInput,
    /// Snapshot of the workspace the task runs against.
    pub workspace: Workspace,
    /// Routing, permission and privacy policy snapshot.
    pub policy: ExecutePolicy,
    /// Client-side execution preferences.
    pub local_preferences: LocalPreferences,
    /// Providers available for this request and their credential status.
    pub providers: Vec<ProviderCredentialInfo>,
    /// The assembled context for the task.
    pub context: ExecuteContext,
}

/// The user's request: prompt text, optional command and explicit model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct TaskInput {
    /// The prompt text.
    pub text: String,
    /// Explicit task command, overriding intent detection when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<TaskIntent>,
    /// Explicit model to use, bypassing routing when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub explicit_model: Option<ModelReference>,
}

/// Snapshot of the workspace a task runs against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Workspace {
    /// Absolute path to the workspace root.
    pub root: PathBuf,
    /// Current git branch, if the workspace is a git repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub git_branch: Option<String>,
    /// Current git diff, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub git_diff: Option<String>,
    /// Paths the user explicitly referenced.
    pub referenced_paths: Vec<PathBuf>,
    /// The file the user is actively editing, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_file: Option<PathBuf>,
}

/// The deterministic policy snapshot the client sends with a task.
///
/// `version` is the schema version of the snapshot; `source` records how it was
/// assembled (for example `merged`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecutePolicy {
    /// Schema version of the policy snapshot.
    pub version: u32,
    /// How the snapshot was assembled, for example `merged`.
    pub source: String,
    /// Routing rules, default model and fallbacks.
    pub routing: ExecuteRoutingPolicy,
    /// Tool permission modes.
    pub permissions: ExecutePermissions,
    /// Privacy constraints on context.
    pub privacy: ExecutePrivacy,
}

/// Routing portion of the policy snapshot.
///
/// `fallbacks` maps a primary model to the ordered models to try if it is
/// unavailable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecuteRoutingPolicy {
    /// Model used when no rule matches.
    pub default_model: ModelReference,
    /// Ordered routing rules.
    pub rules: Vec<ExecuteRoutingRule>,
    /// Per-model ordered fallbacks, keyed by the primary model.
    pub fallbacks: HashMap<ModelReference, Vec<ModelReference>>,
}

/// A single routing rule: match criteria and the model to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecuteRoutingRule {
    /// Criteria that select this rule.
    #[serde(rename = "match")]
    pub match_criteria: RuleMatch,
    /// Model to use when the rule matches.
    #[serde(rename = "use")]
    pub use_model: ModelReference,
}

/// Criteria selecting a routing rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct RuleMatch {
    /// Task type the rule applies to.
    pub task_type: TaskIntent,
    /// Glob of paths the rule applies to, if scoped.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
}

/// Tool permission modes for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecutePermissions {
    /// Permission for reading files.
    pub file_read: PermissionMode,
    /// Permission for writing files.
    pub file_write: PermissionMode,
    /// Permission for running shell commands.
    pub shell: PermissionMode,
    /// Permission for network access.
    pub network: PermissionMode,
}

/// Privacy constraints on the context sent to models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecutePrivacy {
    /// Glob patterns whose contents must not be sent to remote models.
    pub restricted_paths: Vec<String>,
    /// Whether including restricted context in a remote request needs approval.
    pub remote_model_requires_approval_for_restricted_context: bool,
}

/// Client-side execution preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct LocalPreferences {
    /// Apply edits automatically without confirmation.
    pub auto_apply: bool,
    /// Prefer a streaming response.
    pub stream: bool,
    /// Restrict routing to local models only.
    pub local_only: bool,
    /// Forbid any network access for this request.
    pub no_network: bool,
}

/// A provider available to the request and the credential status of its models.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ProviderCredentialInfo {
    /// Provider identifier.
    pub id: Provider,
    /// Models offered by the provider and their credential status.
    pub models: Vec<ProviderModelInfo>,
}

/// A model offered by a provider and whether its credential is available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ProviderModelInfo {
    /// Model name.
    pub model: String,
    /// Whether the model requires an API key.
    pub requires_api_key: bool,
    /// Whether a credential for the model is available to the request.
    pub credential_available: bool,
}

/// The assembled context for a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecuteContext {
    /// Relevant prior conversation messages.
    pub messages: Vec<Message>,
    /// Files included as context.
    pub files: Vec<ContextFile>,
    /// Instruction documents included as context.
    pub instructions: Vec<ContextInstruction>,
    /// Skills made available to the task.
    pub skills: Vec<Skill>,
    /// Prompt template applied to the input, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prompt_template: Option<String>,
}

/// A file included as context, with a content hash for cache validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ContextFile {
    /// Path of the file.
    pub path: PathBuf,
    /// File content.
    pub content: String,
    /// Hash of the content, for example `sha256:...`.
    pub content_hash: String,
}

/// An instruction document included as context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ContextInstruction {
    /// Where the instruction came from, for example `SMISTA.md`.
    pub source: String,
    /// The instruction content.
    pub content: String,
}

/// Outcome of executing a task.
///
/// `message` is the assistant's reply on completion; it may be absent when the
/// task is awaiting an approval instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ExecuteResponse {
    /// Terminal status of the execution.
    pub status: ExecutionStatus,
    /// The assistant's reply, when the task produced one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<Message>,
    /// How the task was routed.
    pub routing: RoutingOutcome,
    /// What context was included and excluded.
    pub context: ContextOutcome,
    /// Token usage and cost for the execution.
    pub usage: Usage,
    /// Identifier of the recorded trace.
    pub trace_id: String,
}

/// Terminal status of a task execution.
///
/// Each variant serializes to its snake_case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ExecutionStatus {
    /// The task completed and produced an assistant message.
    Completed,
    /// The task is paused awaiting an approval decision.
    PendingApproval,
}

/// How a task was routed to a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct RoutingOutcome {
    /// Detected task type that drove routing.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Description of the routing rule that matched, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub matched_rule: Option<String>,
    /// Whether a fallback model was used.
    pub fallback_used: bool,
    /// Whether an explicit model override was used.
    pub override_used: bool,
}

/// What context the router included and excluded for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ContextOutcome {
    /// Human-readable descriptions of included context.
    pub included: Vec<String>,
    /// Human-readable descriptions of excluded context.
    pub excluded: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `/execute` request body from the API specification.
    const SPEC_REQUEST: &str = r#"{
        "input": { "text": "refactor the auth middleware", "command": "edit", "explicit_model": null },
        "workspace": {
            "root": "/Users/christian/project",
            "git_branch": "main",
            "git_diff": "...",
            "referenced_paths": ["src/auth/middleware.rs"],
            "active_file": null
        },
        "policy": {
            "version": 1,
            "source": "merged",
            "routing": {
                "default_model": "anthropic/claude-sonnet",
                "rules": [
                    { "match": { "task_type": "edit", "path": "src/auth/**" }, "use": "anthropic/claude-sonnet" }
                ],
                "fallbacks": {
                    "anthropic/claude-sonnet": ["openai/gpt-5.5-thinking", "ollama/qwen2.5-coder"]
                }
            },
            "permissions": { "file_read": "allow", "file_write": "ask", "shell": "ask", "network": "deny" },
            "privacy": {
                "restricted_paths": [".env", "secrets/**", "target/**"],
                "remote_model_requires_approval_for_restricted_context": true
            }
        },
        "local_preferences": { "auto_apply": false, "stream": true, "local_only": false, "no_network": false },
        "providers": [
            { "id": "anthropic", "models": [ { "model": "claude-sonnet", "requires_api_key": true, "credential_available": true } ] },
            { "id": "ollama", "models": [ { "model": "qwen2.5-coder", "requires_api_key": false, "credential_available": true } ] }
        ],
        "context": {
            "messages": [ { "role": "user", "content": "Previous relevant message..." } ],
            "files": [ { "path": "src/auth/middleware.rs", "content": "...", "content_hash": "sha256:..." } ],
            "instructions": [ { "source": "SMISTA.md", "content": "..." } ],
            "skills": [],
            "prompt_template": null
        }
    }"#;

    #[test]
    fn should_deserialize_spec_execute_request() {
        let request: ExecuteRequest = serde_json::from_str(SPEC_REQUEST).unwrap();

        assert_eq!(request.input.command, Some(TaskIntent::Edit));
        assert_eq!(request.input.explicit_model, None);
        assert_eq!(
            request.workspace.root,
            PathBuf::from("/Users/christian/project")
        );
        assert_eq!(request.workspace.active_file, None);
        assert_eq!(request.policy.version, 1);
        assert_eq!(
            request.policy.routing.default_model,
            "anthropic/claude-sonnet".parse().unwrap()
        );
        assert_eq!(request.policy.permissions.network, PermissionMode::Deny);
        assert!(
            request
                .policy
                .privacy
                .remote_model_requires_approval_for_restricted_context
        );
        assert_eq!(
            request.policy.routing.fallbacks
                [&"anthropic/claude-sonnet".parse::<ModelReference>().unwrap()],
            vec![
                "openai/gpt-5.5-thinking".parse().unwrap(),
                "ollama/qwen2.5-coder".parse().unwrap(),
            ]
        );
        assert!(request.context.skills.is_empty());
        assert_eq!(request.providers.len(), 2);
    }

    #[test]
    fn should_roundtrip_spec_execute_request() {
        let request: ExecuteRequest = serde_json::from_str(SPEC_REQUEST).unwrap();
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<ExecuteRequest>(&json).unwrap(),
            request
        );
    }

    #[test]
    fn should_rename_match_and_use_on_routing_rule() {
        let rule = ExecuteRoutingRule {
            match_criteria: RuleMatch {
                task_type: TaskIntent::Review,
                path: None,
            },
            use_model: "openai/gpt-5.5-thinking".parse().unwrap(),
        };
        let value = serde_json::to_value(&rule).unwrap();
        assert_eq!(value["match"]["task_type"], "review");
        assert_eq!(value["use"], "openai/gpt-5.5-thinking");
        assert!(value["match"].get("path").is_none());
    }

    #[test]
    fn should_deserialize_spec_execute_response() {
        let json = r#"{
            "status": "completed",
            "message": { "role": "assistant", "content": "..." },
            "routing": {
                "task_type": "edit",
                "provider": "anthropic",
                "model": "claude-sonnet",
                "matched_rule": "edit + src/auth/** -> anthropic/claude-sonnet",
                "fallback_used": false,
                "override_used": false
            },
            "context": {
                "included": ["src/auth/middleware.rs", "SMISTA.md", "current git diff"],
                "excluded": [".env", "secrets/**"]
            },
            "usage": { "input_tokens": 1200, "output_tokens": 500, "estimated_cost": "0.08", "currency": "USD" },
            "trace_id": "trace:xyz"
        }"#;
        let response: ExecuteResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.status, ExecutionStatus::Completed);
        assert_eq!(response.routing.provider, Provider::Anthropic);
        assert_eq!(response.usage.input_tokens, Some(1200));
        assert_eq!(response.trace_id, "trace:xyz");
    }

    #[test]
    fn should_serialize_execution_status_as_snake_case() {
        assert_eq!(
            serde_json::to_value(ExecutionStatus::PendingApproval).unwrap(),
            serde_json::json!("pending_approval")
        );
    }
}

//! Request and response bodies for running a task in a session.
//!
//! [`ExecuteRequest`] is the input to `POST /sessions/{id}/execute` and,
//! unchanged, to `/stream` and `/preview`: the user's input, the workspace
//! snapshot, the deterministic policy, local preferences, and the local
//! [`Attachments`] (files, instructions and skills)
//! the router cannot read for itself. Session history, memory and the assembled
//! context are never sent — the router owns them and recalls them from storage.
//!
//! [`TurnResponse`] is the outcome of one turn, shared by `/execute` and
//! `/continue`: a `completed` turn carrying the assistant message, or a
//! continuation the client must act on — a tool to run ([`ToolRequest`]) or an
//! approval to decide ([`PendingApproval`]) — before the run proceeds. See the
//! execution protocol reference for the full interaction model.
//!
//! The `policy` block is the deterministic policy the CLI assembles from its
//! config layers and sends verbatim: it embeds the same
//! [`ClassificationConfig`](crate::policy::ClassificationConfig),
//! [`RoutingPolicy`](crate::policy::RoutingPolicy),
//! [`ToolsConfig`](crate::policy::ToolsConfig) and
//! [`PrivacyPolicy`](crate::policy::PrivacyPolicy) the router evaluates, so
//! there is one policy vocabulary instead of a separate, lossy wire model.
//!
//! Provider credentials are never part of these bodies; they travel as request
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

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{ApiErrorBody, PlainRecord, SealedRecord};
use crate::intent::TaskIntent;
use crate::message::Message;
use crate::model::{ModelReference, Provider};
use crate::policy::{
    Classification, ClassificationConfig, PrivacyPolicy, RoutingPolicy, ToolsConfig,
};
use crate::skill::Skill;
use crate::usage::Usage;

/// Full input to executing, streaming, or previewing a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ExecuteRequest {
    /// What the user is asking for.
    pub input: TaskInput,
    /// Snapshot of the workspace the task runs against.
    pub workspace: Workspace,
    /// Classification, routing, permission and privacy policy snapshot.
    pub policy: ExecutePolicy,
    /// Client-side execution preferences.
    pub local_preferences: LocalPreferences,
    /// Local content the router cannot read for itself.
    pub attachments: Attachments,
}

/// The user's request: prompt text, optional command and explicit model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct TaskInput {
    /// The prompt text.
    pub text: String,
    /// Explicit task command, overriding intent detection when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub command: Option<TaskIntent>,
    /// Explicit model to use, bypassing routing when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub explicit_model: Option<ModelReference>,
}

/// Snapshot of the workspace a task runs against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Workspace {
    /// Absolute path to the workspace root.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub root: PathBuf,
    /// Current git branch, if the workspace is a git repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub git_branch: Option<String>,
    /// Current git diff, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub git_diff: Option<String>,
    /// Paths the user explicitly referenced.
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<String>))]
    pub referenced_paths: Vec<PathBuf>,
    /// The file the user is actively editing, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>, nullable = true))]
    pub active_file: Option<PathBuf>,
}

/// The deterministic policy snapshot the client sends with a task.
///
/// `version` is the schema version of the snapshot; `source` records how it was
/// assembled (for example `merged`). The `classification`, `routing`, `tools`
/// and `privacy` blocks are the canonical [`crate::policy`] types — the same
/// vocabulary the router evaluates and the CLI loads from `config.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ExecutePolicy {
    /// Schema version of the policy snapshot.
    pub version: u32,
    /// How the snapshot was assembled, for example `merged`.
    pub source: String,
    /// Intent-classification rules and the default intent.
    pub classification: ClassificationConfig,
    /// Routing rules and default route.
    pub routing: RoutingPolicy,
    /// Tool permission modes, keyed by tool name.
    pub tools: ToolsConfig,
    /// Privacy constraints on the context sent to models.
    pub privacy: PrivacyPolicy,
}

/// Client-side execution preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
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

/// Local content the client supplies because the router has no filesystem.
///
/// Every file, instruction and skill the task may need from disk travels here.
/// History, memory and the assembled context are not part of this — the router
/// owns them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct Attachments {
    /// Explicit `@path` files included with their content.
    pub files: Vec<ContextFile>,
    /// Instruction documents the client read from disk.
    pub instructions: Vec<ContextInstruction>,
    /// Skills the user explicitly invoked, with their full `SKILL.md` body.
    ///
    /// Authoritative: routing rules may match on them by name, and their
    /// instructions are added to the model preamble. The router never infers
    /// this set from the prompt.
    #[serde(default)]
    pub invoked_skills: Vec<Skill>,
    /// Skills offered for the serving model to activate by reading them.
    ///
    /// Same shape as the invoked skills, but distinct by role: they never
    /// influence routing, and the model decides whether to apply one. The full
    /// instructions travel up front, so an activated skill needs no follow-up
    /// fetch.
    #[serde(default)]
    pub available_skills: Vec<Skill>,
}

/// A file the client attached, with a content hash and whether it is required.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ContextFile {
    /// Path of the file.
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub path: PathBuf,
    /// File content.
    pub content: String,
    /// Hash of the content, for example `sha256:...`.
    pub content_hash: String,
    /// Whether the file is required (referenced or route-driving) and so cannot
    /// be dropped, as opposed to discardable supplementary context.
    pub required: bool,
}

/// An instruction document the client attached as context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ContextInstruction {
    /// Where the instruction came from, for example `SMISTA.md`.
    pub source: String,
    /// The instruction content.
    pub content: String,
}

/// The outcome of one turn, returned by `/execute` and `/continue`.
///
/// Internally tagged by `status`. A [`Completed`](Self::Completed) turn carries
/// the assistant message and routing explanation; every other variant is a
/// continuation the client must act on before resuming the run with
/// `/continue`, or a terminal [`Error`](Self::Error).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "status", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum TurnResponse {
    /// The model finished; an assistant message is included.
    ///
    /// Boxed because this is by far the largest variant.
    Completed(Box<CompletedTurn>),
    /// The model requested one or more client-executed tools.
    AwaitingTool {
        /// The tool calls the client must run, correlated by `call_id`.
        tool_requests: Vec<ToolRequest>,
        /// Identifier of the recorded trace.
        trace_id: String,
    },
    /// The router needs a yes/no decision with no tool to run.
    AwaitingApproval {
        /// The decision the client must obtain.
        approval: PendingApproval,
        /// Identifier of the recorded trace.
        trace_id: String,
    },
    /// The router needs sealed history opened to build the prompt.
    AwaitingDecrypt {
        /// Records the client must decrypt, correlated by `record_id`.
        records: Vec<SealedRecord>,
        /// Identifier of the recorded trace.
        trace_id: String,
    },
    /// The router needs its own output sealed before it can be persisted.
    AwaitingEncrypt {
        /// Records the client must encrypt, correlated by `record_id`.
        records: Vec<PlainRecord>,
        /// Identifier of the recorded trace.
        trace_id: String,
    },
    /// A terminal error ended the turn.
    Error {
        /// The structured error body.
        error: ApiErrorBody,
    },
}

/// The payload of a [`completed`](TurnResponse::Completed) turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct CompletedTurn {
    /// The assistant's reply.
    pub message: Message,
    /// How the task was classified.
    pub classification: Classification,
    /// How the task was routed.
    pub routing: RoutingOutcome,
    /// What context was included and excluded.
    pub context: ContextOutcome,
    /// Token usage and cost for the turn.
    pub usage: Usage,
    /// Identifier of the recorded trace.
    pub trace_id: String,
}

/// A tool the client must execute on the user's machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ToolRequest {
    /// Identifier correlating this call with its later result.
    pub call_id: String,
    /// Name of the tool to invoke.
    pub name: String,
    /// Arguments for the tool, as a provider-agnostic JSON value.
    #[cfg_attr(feature = "openapi", schema(value_type = Object, nullable = true))]
    pub arguments: serde_json::Value,
    /// Whether the user must approve the call before it runs.
    pub requires_approval: ToolApproval,
}

/// Whether a client-executed tool needs the user's approval first.
///
/// A tool denied by policy never reaches the client, so `deny` is not a value
/// here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ToolApproval {
    /// Run the tool without confirmation.
    Allow,
    /// Confirm with the user before running the tool.
    Ask,
}

/// A decision the router needs that has no tool to execute.
///
/// Raised for disclosures and limits — for example sending context to a remote
/// model under an `ask` privacy mode, or confirming a cost ceiling. The
/// `detail` shape depends on `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct PendingApproval {
    /// Identifier the approval decision refers to.
    pub approval_id: String,
    /// What kind of decision is being asked for.
    pub kind: ApprovalKind,
    /// Machine-readable context for the decision; its shape depends on `kind`.
    #[cfg_attr(feature = "openapi", schema(value_type = Object, nullable = true))]
    pub detail: serde_json::Value,
}

/// The kind of a [`PendingApproval`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ApprovalKind {
    /// Disclosing context to a remote provider.
    RemoteDisclosure,
    /// Confirming a per-task cost ceiling would be exceeded.
    CostLimit,
    /// Accepting or rejecting a generated plan before execution begins. The
    /// decision has no tool to run, so it travels as a standalone approval.
    Plan,
}

/// How a task was routed to a model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct RoutingOutcome {
    /// Detected task type that drove routing.
    pub task_type: TaskIntent,
    /// Provider that served the task.
    pub provider: Provider,
    /// Model that served the task.
    pub model: String,
    /// Description of the routing rule that matched, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub matched_rule: Option<String>,
    /// Whether a fallback model was used.
    pub fallback_used: bool,
    /// Whether an explicit model override was used.
    pub override_used: bool,
}

/// What context the router included and excluded for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ContextOutcome {
    /// Human-readable descriptions of included context.
    pub included: Vec<String>,
    /// Human-readable descriptions of excluded context.
    pub excluded: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::EncryptedPayload;
    use crate::effort::Effort;
    use crate::policy::{IntentSource, PermissionMode};

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
            "classification": {
                "default_intent": "chat",
                "rules": [
                    { "intent": "review", "priority": 10, "keywords": ["review", "audit"], "requires_any_context": ["git_diff"] }
                ]
            },
            "routing": {
                "default": {
                    "model": "anthropic/claude-sonnet",
                    "fallbacks": ["openai/gpt-5.5-thinking", "ollama/qwen2.5-coder"]
                },
                "rules": [
                    {
                        "name": "auth edits use Claude",
                        "priority": 30,
                        "effort": "high",
                        "intent": "edit",
                        "paths": ["src/auth/**"],
                        "model": "anthropic/claude-sonnet",
                        "fallbacks": ["openai/gpt-5.5-thinking"]
                    }
                ]
            },
            "tools": { "permissions": { "file_read": "allow", "file_write": "ask", "shell": "ask", "network": "deny" } },
            "privacy": {
                "restricted_paths": [".env", "secrets/**", "target/**"],
                "remote": { "mode": "ask", "blocked_paths": [] },
                "local": { "mode": "allow" }
            }
        },
        "local_preferences": { "auto_apply": false, "stream": true, "local_only": false, "no_network": false },
        "attachments": {
            "files": [ { "path": "src/auth/middleware.rs", "content": "...", "content_hash": "sha256:...", "required": true } ],
            "instructions": [ { "source": "SMISTA.md", "content": "..." } ],
            "invoked_skills": [ { "name": "code-review", "content": "Report findings by severity." } ],
            "available_skills": [ { "name": "changelog", "content": "Summarize changes under a heading." } ]
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
        assert_eq!(request.policy.version, 1);
        assert_eq!(
            request.policy.classification.default_intent,
            TaskIntent::Chat
        );
        assert_eq!(request.policy.classification.rules.len(), 1);
        assert_eq!(
            request.policy.classification.rules[0].intent,
            TaskIntent::Review
        );
        let default_route = request.policy.routing.default.as_ref().unwrap();
        assert_eq!(
            default_route.model,
            "anthropic/claude-sonnet".parse().unwrap()
        );
        assert_eq!(
            request.policy.tools.mode_for("network"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(request.policy.privacy.remote.mode(), PermissionMode::Ask);
        let rule = &request.policy.routing.rules[0];
        assert_eq!(rule.intent, Some(TaskIntent::Edit));
        assert_eq!(rule.effort, Effort::High);
        let file = &request.attachments.files[0];
        assert_eq!(file.path, PathBuf::from("src/auth/middleware.rs"));
        assert!(file.required);
        assert_eq!(request.attachments.invoked_skills.len(), 1);
        assert_eq!(request.attachments.invoked_skills[0].name, "code-review");
        assert_eq!(request.attachments.available_skills.len(), 1);
        assert_eq!(request.attachments.available_skills[0].name, "changelog");
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
    fn should_deserialize_completed_turn() {
        let json = r#"{
            "status": "completed",
            "message": { "role": "assistant", "content": "..." },
            "classification": { "intent": "edit", "source": "inferred", "reason": "keyword matched", "confidence": "high" },
            "routing": {
                "task_type": "edit",
                "provider": "anthropic",
                "model": "claude-sonnet",
                "matched_rule": "edit + src/auth/** -> anthropic/claude-sonnet",
                "fallback_used": false,
                "override_used": false
            },
            "context": { "included": ["src/auth/middleware.rs"], "excluded": [".env"] },
            "usage": { "input_tokens": 1200, "output_tokens": 500, "estimated_cost": "0.08", "currency": "USD" },
            "trace_id": "trace:xyz"
        }"#;
        let response: TurnResponse = serde_json::from_str(json).unwrap();

        let TurnResponse::Completed(completed) = response else {
            panic!("expected a completed turn");
        };
        assert_eq!(completed.classification.intent, TaskIntent::Edit);
        assert_eq!(completed.classification.source, IntentSource::Inferred);
        assert_eq!(completed.routing.provider, Provider::Anthropic);
        assert_eq!(completed.usage.input_tokens, Some(1200));
        assert_eq!(completed.trace_id, "trace:xyz");
    }

    #[test]
    fn should_serialize_awaiting_tool_with_status_tag() {
        let response = TurnResponse::AwaitingTool {
            tool_requests: vec![ToolRequest {
                call_id: "c1".to_string(),
                name: "shell".to_string(),
                arguments: serde_json::json!({ "command": "cargo test" }),
                requires_approval: ToolApproval::Ask,
            }],
            trace_id: "trace:xyz".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            serde_json::json!({
                "status": "awaiting_tool",
                "tool_requests": [
                    { "call_id": "c1", "name": "shell", "arguments": { "command": "cargo test" }, "requires_approval": "ask" }
                ],
                "trace_id": "trace:xyz"
            })
        );
    }

    #[test]
    fn should_serialize_awaiting_approval_with_kind() {
        let response = TurnResponse::AwaitingApproval {
            approval: PendingApproval {
                approval_id: "a1".to_string(),
                kind: ApprovalKind::RemoteDisclosure,
                detail: serde_json::json!({ "provider": "anthropic" }),
            },
            trace_id: "trace:xyz".to_string(),
        };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["status"], "awaiting_approval");
        assert_eq!(value["approval"]["kind"], "remote_disclosure");
    }

    #[test]
    fn should_roundtrip_error_turn() {
        let response = TurnResponse::Error {
            error: ApiErrorBody {
                code: "no_route".to_string(),
                message: "No routing rule matched.".to_string(),
                details: None,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<TurnResponse>(&json).unwrap(),
            response
        );
    }

    #[test]
    fn should_roundtrip_awaiting_decrypt() {
        let response = TurnResponse::AwaitingDecrypt {
            records: vec![SealedRecord {
                record_id: "session_message:0194".to_string(),
                envelope: EncryptedPayload {
                    version: 1,
                    algorithm: "xchacha20poly1305".to_string(),
                    key_id: "kf_ab12".to_string(),
                    nonce: "bm9uY2U".to_string(),
                    ciphertext: "Y2lwaGVydGV4dA".to_string(),
                },
            }],
            trace_id: "trace:xyz".to_string(),
        };
        let value = serde_json::to_value(&response).unwrap();
        assert_eq!(value["status"], "awaiting_decrypt");
        assert_eq!(
            serde_json::from_value::<TurnResponse>(value).unwrap(),
            response
        );
    }

    #[test]
    fn should_serialize_tool_approval_as_snake_case() {
        assert_eq!(
            serde_json::to_value(ToolApproval::Allow).unwrap(),
            serde_json::json!("allow")
        );
        assert_eq!(
            serde_json::to_value(ToolApproval::Ask).unwrap(),
            serde_json::json!("ask")
        );
    }
}

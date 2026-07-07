//! All Commands with their payloads sent by the UI to the router client to be executed on the smista-router.

use std::collections::HashMap;
use std::path::PathBuf;

use smista_sdk::core::model::ModelReference;
use uuid::Uuid;

/// Commands are sent by the UI to the router client to be executed on the smista-router.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Router command variants are scaffolded before TUI routing is wired."
    )
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    /// Execute a user prompt through the router.
    Execute {
        /// User prompt to be executed by the router. This is the main input for the router to process.
        prompt: String,
        /// Files context loaded by the user. Maps file paths to their content.
        files: HashMap<PathBuf, String>,
        /// Whether planning mode is enabled.
        plan: bool,
        /// Explicit model to use, bypassing routing when set.
        explicit_model: Option<ModelReference>,
    },
    /// Continue paused or streaming execution with user-visible input.
    Continue(ContinueExecution),
    /// Interrupt any active run and clear the current session.
    Clear,
    /// List models available on the router for this user.
    ListModels,
    /// List providers available on the router for this user.
    ListProviders,
    /// List sessions on the router for this user.
    ListSessions,
    /// Resume a session on the router for this user.
    ResumeSession(Uuid),
    /// Get the current usage statistics for this session.
    GetUsage,
    /// Get the execution trace for this session.
    GetTrace,
    /// Preview how the router would execute a user prompt.
    Preview {
        /// User prompt to preview through the router.
        prompt: String,
        /// Files context loaded by the user. Maps file paths to their content.
        files: HashMap<PathBuf, String>,
        /// Whether planning mode is enabled.
        plan: bool,
        /// Explicit model to use, bypassing routing when set.
        explicit_model: Option<ModelReference>,
    },
    /// Get the router health status.
    GetRouterStatus,
}

/// User-visible continuation actions for a paused router run.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Continuation variants are scaffolded before TUI routing is wired."
    )
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContinueExecution {
    /// Submit results for tools the client executed.
    ToolResults {
        /// One entry per executed tool call.
        results: Vec<ToolResult>,
    },
    /// Submit decisions for approvals that have no tool to run.
    ApprovalDecisions {
        /// One entry per pending approval.
        decisions: Vec<ApprovalDecision>,
    },
    /// Inject new user messages into an active run.
    Inject {
        /// User messages to queue for the router.
        messages: Vec<UserMessage>,
    },
    /// Abort an active run without additional input.
    Break,
}

/// Result of one tool execution as entered back into the UI protocol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Identifier matching the original tool request.
    pub call_id: String,
    /// Human-readable tool output.
    pub content: String,
    /// Whether the tool failed.
    pub is_error: bool,
}

/// User decision for one pending approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalDecision {
    /// Identifier of the approval or tool request being decided.
    pub id: String,
    /// Whether the user approved or rejected the request.
    pub outcome: ApprovalOutcome,
    /// Whether this decision applies once or to the matching session alias.
    pub scope: ApprovalScope,
    /// Optional human-readable reason.
    pub reason: Option<String>,
}

/// Result of an approval prompt.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Approval outcomes are scaffolded before TUI routing is wired."
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    /// The user approved the request.
    Approved,
    /// The user rejected the request.
    Rejected,
}

/// Persistence scope for an approval decision.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "Approval scopes are scaffolded before TUI routing is wired."
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalScope {
    /// Apply the approval decision only to the current request.
    Once,
    /// Apply the approval decision to the prompt's wildcard alias for this session.
    AlwaysForSession,
}

/// User message injected into an active run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMessage {
    /// Message text.
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_decision_can_apply_once_with_rejection() {
        let decision = ApprovalDecision {
            id: "approval-1".to_owned(),
            outcome: ApprovalOutcome::Rejected,
            scope: ApprovalScope::Once,
            reason: Some("not now".to_owned()),
        };

        assert_eq!(decision.outcome, ApprovalOutcome::Rejected);
        assert_eq!(decision.scope, ApprovalScope::Once);
    }

    #[test]
    fn approval_decision_carries_session_scope() {
        let decision = ApprovalDecision {
            id: "approval-1".to_owned(),
            outcome: ApprovalOutcome::Approved,
            scope: ApprovalScope::AlwaysForSession,
            reason: Some("ok for session".to_owned()),
        };

        assert_eq!(decision.outcome, ApprovalOutcome::Approved);
        assert_eq!(decision.scope, ApprovalScope::AlwaysForSession);
    }
}

use std::collections::HashMap;
use std::path::PathBuf;

use uuid::Uuid;

/// Commands are sent by the UI to the router client to be executed on the smista-router.
#[expect(
    dead_code,
    reason = "Router command variants are scaffolded before TUI routing is wired."
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
    },
    /// Continue paused or streaming execution with user-visible input.
    Continue(ContinueExecution),
    /// Interrupt any active run and clear the current session.
    Clear,
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

/// Messages are sent by the router client to the UI to notify about the status of the execution.
#[expect(
    dead_code,
    reason = "Router message variants are defined before real router execution is wired."
)]
#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// The router completed an assistant turn.
    AssistantTurn(AssistantTurn),
    /// The router streamed a generated content chunk.
    StreamedContentChunk(String),
    /// The router is waiting for the user to approve or reject an action.
    ApprovalPrompt(ApprovalPrompt),
    /// The router is waiting for the client to run tools.
    ToolRequestPrompt(Vec<ToolRequestPrompt>),
    /// The router returned the user's sessions.
    SessionsList(Vec<SessionListItem>),
    /// The router returned a resumed session.
    ResumedSession(ResumedSession),
    /// The router returned usage statistics.
    Usage(UsageSummary),
    /// The router returned an execution trace.
    Trace(TraceSummary),
    /// The router returned a preview.
    Preview(PreviewSummary),
    /// The router returned its health status.
    RouterStatus(RouterStatus),
    /// Error raised by the router during execution. This is sent to the UI to notify the user about the error.
    Error(String),
    /// The router has no active turn.
    Idle,
    /// The router is processing a turn. Sent to the UI whenever a request is sent to the router and a response is awaited.
    Thinking,
}

/// Assistant turn data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantTurn {
    /// Assistant text to render.
    pub message: String,
    /// Trace identifier, if the router reported one.
    pub trace_id: Option<String>,
}

/// Approval prompt data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalPrompt {
    /// Identifier used when replying with an [`ApprovalDecision`].
    pub id: String,
    /// Short approval title.
    pub title: String,
    /// Human-readable detail to show in the prompt.
    pub detail: String,
    /// Session-policy wildcard alias, for example `git commit *`.
    pub wildcard_alias: Option<String>,
}

/// Tool request data reduced for UI approval and execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRequestPrompt {
    /// Identifier used when sending a [`ToolResult`] or [`ApprovalDecision`].
    pub call_id: String,
    /// Tool name.
    pub name: String,
    /// Human-readable call summary.
    pub detail: String,
    /// Session-policy wildcard alias, for example `git commit *`.
    pub wildcard_alias: Option<String>,
}

/// One session row for the UI session list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionListItem {
    /// Session identifier.
    pub id: Uuid,
    /// Session title.
    pub title: Option<String>,
    /// Session grouping scope.
    pub scope: Option<String>,
    /// Last update timestamp already formatted for display.
    pub updated_at: String,
}

/// Resumed session data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumedSession {
    /// Session identifier.
    pub id: Uuid,
    /// Session title.
    pub title: String,
    /// Message transcript.
    pub messages: Vec<SessionMessage>,
}

/// One resumed session transcript row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMessage {
    /// Message role rendered by the UI.
    pub role: String,
    /// Message text rendered by the UI.
    pub content: String,
}

/// Usage data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSummary {
    /// Main usage line.
    pub total: String,
    /// Per-model usage lines.
    pub by_model: Vec<String>,
    /// Per-task usage lines.
    pub by_task_type: Vec<String>,
}

/// Trace data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceSummary {
    /// Human-readable trace event lines.
    pub events: Vec<String>,
}

/// Preview data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSummary {
    /// Task type that would be routed.
    pub task_type: String,
    /// Provider that would be used.
    pub provider: String,
    /// Model that would be used.
    pub model: String,
    /// Required permission lines.
    pub required_permissions: Vec<String>,
}

/// Router health data reduced for UI rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterStatus {
    /// Router status string.
    pub status: String,
    /// Router version string.
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_prompt_carries_wildcard_alias() {
        let msg = Msg::ApprovalPrompt(ApprovalPrompt {
            id: "approval-1".to_owned(),
            title: "Run git command".to_owned(),
            detail: "git commit -a -m something".to_owned(),
            wildcard_alias: Some("git commit *".to_owned()),
        });

        let Msg::ApprovalPrompt(prompt) = msg else {
            panic!("approval prompt expected");
        };
        assert_eq!(prompt.wildcard_alias.as_deref(), Some("git commit *"));
    }

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

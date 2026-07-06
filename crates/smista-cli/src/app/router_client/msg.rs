//! All types of messages with their payloads sent by the router client to the UI to notify about the status of the execution.

use uuid::Uuid;

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
    /// Identifier used when replying with an
    /// [`ApprovalDecision`](crate::app::router_client::cmd::ApprovalDecision).
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
    /// Identifier used when sending a [`ToolResult`](crate::app::router_client::cmd::ToolResult)
    /// or [`ApprovalDecision`](crate::app::router_client::cmd::ApprovalDecision).
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
}

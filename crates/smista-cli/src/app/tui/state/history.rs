//! Renderable entries kept in the main transcript pane.

use crate::app::router_client::msg::{ApprovalPrompt, PreviewSummary, TraceEvent};

/// Defines one renderable history block in the main pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryEntry {
    /// A user-authored prompt.
    UserMessage(String),
    /// Plain assistant text.
    AssistantMessage(String),
    /// Assistant reasoning text.
    Reasoning(String),
    /// A tool call shown in the transcript.
    ToolCall { name: String, input: String },
    /// A tool result shown in the transcript.
    ToolResult { name: String, output: String },
    /// A diff block.
    Diff {
        /// Added lines.
        added: String,
        /// Removed lines.
        removed: String,
    },
    /// An approval request shown in the transcript.
    ApprovalRequest(ApprovalPrompt),
    /// A trace event shown in the transcript.
    Trace(TraceEvent),
    /// A routing preview shown in the transcript.
    Preview(PreviewSummary),
    /// An error block.
    Error(String),
    /// A short non-error status line.
    Notice(String),
}

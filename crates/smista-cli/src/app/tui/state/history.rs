//! Renderable entries kept in the main transcript pane.

use crate::app::log::AppLogEntry;
use crate::app::router_client::msg::{ApprovalPrompt, PreviewSummary, TraceEvent};

/// Defines one renderable history block in the main pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryEntry {
    /// An approval request shown in the transcript.
    ApprovalRequest(ApprovalPrompt),
    /// Plain assistant text.
    AssistantMessage(String),
    /// A diff block.
    Diff {
        /// Added lines.
        added: String,
        /// Removed lines.
        removed: String,
    },
    /// A log block.
    Log(Vec<AppLogEntry>),
    /// An error block.
    Error(String),
    /// A short non-error status line.
    Notice(String),
    /// A routing preview shown in the transcript.
    Preview(PreviewSummary),
    /// Assistant reasoning text.
    Reasoning(String),
    /// A tool call shown in the transcript.
    ToolCall { name: String, input: String },
    /// A tool result shown in the transcript.
    ToolResult { name: String, output: String },
    /// A trace event shown in the transcript.
    Trace(TraceEvent),
    /// A user-authored prompt.
    UserMessage(String),
}

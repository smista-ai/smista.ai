//! State for an in-progress execution turn.

use crate::app::router_client::msg::{ApprovalPrompt, ToolCallStarted};

/// State for content that has not yet become a durable history entry.
#[derive(Debug)]
pub enum ExecutionTurn {
    /// The assistant is streaming content.
    Streaming {
        /// Content streamed so far.
        content: String,
        /// Reasoning streamed so far.
        reasoning: String,
    },
    /// The router started a tool call.
    ToolCall(ToolCallStarted),
    /// The router is waiting for approval.
    Approval(ApprovalPrompt),
}

impl ExecutionTurn {
    /// Creates an empty streaming turn.
    #[must_use]
    pub fn streaming() -> Self {
        Self::Streaming {
            content: String::new(),
            reasoning: String::new(),
        }
    }

    /// Appends content to a streaming turn.
    pub fn push_content(&mut self, chunk: &str) {
        if let Self::Streaming { content, .. } = self {
            content.push_str(chunk);
        }
    }

    /// Appends reasoning to a streaming turn.
    pub fn push_reasoning(&mut self, chunk: &str) {
        if let Self::Streaming { reasoning, .. } = self {
            reasoning.push_str(chunk);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPROVAL_DETAIL: &str = "run command";
    const APPROVAL_ID: &str = "approval-1";
    const APPROVAL_TITLE: &str = "Run command";
    const CONTENT_CHUNK: &str = "hello";
    const REASONING_CHUNK: &str = "thinking";
    const TOOL_CALL_ID: &str = "call-1";
    const TOOL_NAME: &str = "shell";

    #[test]
    fn streaming_turn_accumulates_content_and_reasoning() {
        let mut turn = ExecutionTurn::streaming();

        turn.push_content(CONTENT_CHUNK);
        turn.push_reasoning(REASONING_CHUNK);

        let ExecutionTurn::Streaming { content, reasoning } = turn else {
            panic!("streaming turn expected");
        };
        assert_eq!(content, CONTENT_CHUNK);
        assert_eq!(reasoning, REASONING_CHUNK);
    }

    #[test]
    fn non_streaming_turns_ignore_stream_chunks() {
        let mut tool_call = ExecutionTurn::ToolCall(ToolCallStarted {
            call_id: TOOL_CALL_ID.to_owned(),
            name: TOOL_NAME.to_owned(),
        });
        tool_call.push_content(CONTENT_CHUNK);
        tool_call.push_reasoning(REASONING_CHUNK);

        let ExecutionTurn::ToolCall(tool_call) = tool_call else {
            panic!("tool call turn expected");
        };
        assert_eq!(tool_call.call_id, TOOL_CALL_ID);
        assert_eq!(tool_call.name, TOOL_NAME);

        let mut approval = ExecutionTurn::Approval(ApprovalPrompt {
            id: APPROVAL_ID.to_owned(),
            title: APPROVAL_TITLE.to_owned(),
            detail: APPROVAL_DETAIL.to_owned(),
            wildcard_alias: None,
        });
        approval.push_content(CONTENT_CHUNK);
        approval.push_reasoning(REASONING_CHUNK);

        let ExecutionTurn::Approval(approval) = approval else {
            panic!("approval turn expected");
        };
        assert_eq!(approval.id, APPROVAL_ID);
        assert_eq!(approval.title, APPROVAL_TITLE);
    }
}

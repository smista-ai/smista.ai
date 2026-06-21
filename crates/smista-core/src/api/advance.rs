//! Request body for advancing an in-flight run.
//!
//! [`ContinueRequest`] is the body of `POST /sessions/{id}/continue`: a bundle
//! that answers whatever continuation the previous turn returned. Every field
//! is optional, and a single request may carry several at once — for example
//! tool results plus a queued user message. It replaces the standalone approval
//! endpoint: a tool's approval rides the [`ToolResult::decision`], and a
//! decision with no tool to run rides [`ContinueRequest::approval_decisions`].
//!
//! # Examples
//!
//! ```
//! use smista_core::api::ContinueRequest;
//!
//! let bundle: ContinueRequest = serde_json::from_str("{}").unwrap();
//! assert!(bundle.tool_results.is_empty());
//! assert!(!bundle.interrupt);
//! ```

use serde::{Deserialize, Serialize};

use super::{ApprovalDecision, PlainRecord, SealedRecord};

/// Bundle that advances an in-flight run after a continuation.
///
/// Every field defaults to empty, so a bare `{}` is a valid no-op and a bare
/// `{ "interrupt": true }` aborts the in-flight turn with no other input.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ContinueRequest {
    /// Results of tools the client ran, answering an `awaiting_tool` turn.
    #[serde(default)]
    pub tool_results: Vec<ToolResult>,
    /// Decisions for approvals with no tool to run, answering an
    /// `awaiting_approval` turn.
    #[serde(default)]
    pub approval_decisions: Vec<ApprovalDecisionEntry>,
    /// Records the client decrypted, answering an `awaiting_decrypt` turn.
    #[serde(default)]
    pub decrypted: Vec<PlainRecord>,
    /// Records the client encrypted, answering an `awaiting_encrypt` turn.
    #[serde(default)]
    pub encrypted: Vec<SealedRecord>,
    /// User input typed while the run was working, delivered at this boundary.
    #[serde(default)]
    pub user_messages: Vec<UserMessage>,
    /// Whether to abort the in-flight turn (the Esc path).
    #[serde(default)]
    pub interrupt: bool,
}

/// The result of one client-executed tool, correlated by `call_id`.
///
/// For an `ask`-mode tool the user's decision rides [`Self::decision`], so the
/// approval and the result arrive together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ToolResult {
    /// Identifier matching the originating tool request.
    pub call_id: String,
    /// The tool's output.
    pub content: String,
    /// Whether the tool call failed.
    #[serde(default)]
    pub is_error: bool,
    /// The user's decision for an `ask`-mode tool, if one was required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub decision: Option<ApprovalDecision>,
}

/// A decision for a [`PendingApproval`](super::PendingApproval) with no tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct ApprovalDecisionEntry {
    /// Identifier of the approval the decision applies to.
    pub approval_id: String,
    /// The decision being submitted.
    pub decision: ApprovalDecision,
    /// Optional human-readable reason for the decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reason: Option<String>,
}

/// A message the user typed while the run was working.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct UserMessage {
    /// The message text.
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_empty_bundle() {
        let bundle: ContinueRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(bundle, ContinueRequest::default());
        assert!(bundle.tool_results.is_empty());
        assert!(bundle.approval_decisions.is_empty());
        assert!(bundle.user_messages.is_empty());
        assert!(!bundle.interrupt);
    }

    #[test]
    fn should_deserialize_tool_results_with_decision() {
        let json = r#"{
            "tool_results": [
                { "call_id": "c1", "content": "ok", "is_error": false },
                { "call_id": "c2", "content": "ran", "is_error": false, "decision": "approved" }
            ]
        }"#;
        let bundle: ContinueRequest = serde_json::from_str(json).unwrap();
        assert_eq!(bundle.tool_results.len(), 2);
        assert_eq!(bundle.tool_results[0].decision, None);
        assert_eq!(
            bundle.tool_results[1].decision,
            Some(ApprovalDecision::Approved)
        );
    }

    #[test]
    fn should_default_tool_result_error_flag() {
        let result: ToolResult =
            serde_json::from_str(r#"{ "call_id": "c1", "content": "ok" }"#).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.decision, None);
    }

    #[test]
    fn should_deserialize_interrupt_with_user_message() {
        let json = r#"{ "interrupt": true, "user_messages": [{ "text": "stop, do Y instead" }] }"#;
        let bundle: ContinueRequest = serde_json::from_str(json).unwrap();
        assert!(bundle.interrupt);
        assert_eq!(bundle.user_messages[0].text, "stop, do Y instead");
    }

    #[test]
    fn should_roundtrip_serde() {
        let bundle = ContinueRequest {
            approval_decisions: vec![ApprovalDecisionEntry {
                approval_id: "a1".to_string(),
                decision: ApprovalDecision::Rejected,
                reason: Some("not now".to_string()),
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&bundle).unwrap();
        assert_eq!(
            serde_json::from_str::<ContinueRequest>(&json).unwrap(),
            bundle
        );
    }
}

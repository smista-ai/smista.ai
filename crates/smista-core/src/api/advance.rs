//! Tagged messages that advance an in-flight run.
//!
//! [`ContinueRequest`] is the body of `POST /sessions/{id}/continue`: a single
//! `{ type, data }` message answering whatever continuation the previous turn
//! returned. The router advertises the valid next messages in each
//! [`TurnResponse`](super::TurnResponse)'s `allowed_continuations`;
//! [`ContinueKind`] names them. `break` is always valid while a run is live.
//!
//! Approval for an `ask`-mode tool rides [`ToolResult::decision`]; a decision
//! with no tool to run rides [`ContinueRequest::ApprovalDecisions`]. In an
//! end-to-end-encrypted session, content the client seals travels in the
//! `encrypted` map keyed by [`ContentRef`], so the router dispatches each
//! payload to the right content store.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::ContinueRequest;
//!
//! let msg: ContinueRequest = serde_json::from_str(r#"{ "type": "break" }"#).unwrap();
//! assert_eq!(msg, ContinueRequest::Break);
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ApprovalDecision, ContentRef, EncryptedPayload};

/// A single tagged message that advances an in-flight run.
///
/// Serialized adjacently tagged: `{ "type": <kind>, "data": { … } }`, except the
/// data-less [`Break`](Self::Break), which is just `{ "type": "break" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ContinueRequest {
    /// Results of tools the client ran, answering an `awaiting_tool` turn.
    ToolResults {
        /// One entry per executed call, correlated by `call_id`.
        results: Vec<ToolResult>,
        /// Sealed forms of router-authored rows (the assistant message, tool
        /// arguments) and of the tool results, keyed by [`ContentRef`]. Empty
        /// unless the session is end-to-end encrypted.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        encrypted: BTreeMap<ContentRef, EncryptedPayload>,
    },
    /// Decisions for standalone approvals, answering an `awaiting_approval` turn.
    ApprovalDecisions {
        /// One entry per pending approval.
        decisions: Vec<ApprovalDecisionEntry>,
        /// Sealed router-authored rows (for example a plan snapshot), keyed by
        /// [`ContentRef`]. Empty unless the session is end-to-end encrypted.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        encrypted: BTreeMap<ContentRef, EncryptedPayload>,
    },
    /// Plaintext of records the client opened, answering an `awaiting_decrypt`
    /// turn. Keyed by [`ContentRef`].
    Decrypted {
        /// [`ContentRef`] -> opened plaintext.
        plaintext: BTreeMap<ContentRef, String>,
        /// Sealed forms of router-authored rows the turn asked to seal alongside
        /// the decrypt (for example the run-input bundle), keyed by
        /// [`ContentRef`]. Empty unless the session is end-to-end encrypted.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        encrypted: BTreeMap<ContentRef, EncryptedPayload>,
    },
    /// Ciphertext the client sealed for the encrypt request folded onto a
    /// completed or interrupted turn. Keyed by [`ContentRef`].
    Sealed {
        /// [`ContentRef`] -> sealed envelope.
        encrypted: BTreeMap<ContentRef, EncryptedPayload>,
    },
    /// Mid-run input the user typed: supersede the in-flight turn and continue
    /// with it.
    Inject {
        /// The queued user messages, in order.
        messages: Vec<UserMessage>,
    },
    /// Abort the in-flight turn with no further input (the Esc path).
    Break,
}

/// The kind of a [`ContinueRequest`], as advertised in
/// [`allowed_continuations`](super::TurnResponse).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ContinueKind {
    /// Results for an `awaiting_tool` turn.
    ToolResults,
    /// Decisions for an `awaiting_approval` turn.
    ApprovalDecisions,
    /// Opened plaintext for an `awaiting_decrypt` turn.
    Decrypted,
    /// Sealed ciphertext for a folded encrypt request.
    Sealed,
    /// Mid-run user input.
    Inject,
    /// Abort the in-flight turn. Always valid while a run is live.
    Break,
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
    /// Sealed form for storage, present only for an end-to-end-encrypted
    /// session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ciphertext: Option<EncryptedPayload>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::EncryptedPayload;

    fn envelope() -> EncryptedPayload {
        EncryptedPayload {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    #[test]
    fn should_tag_tool_results_with_type_and_data() {
        let msg = ContinueRequest::ToolResults {
            results: vec![ToolResult {
                call_id: "c1".to_string(),
                content: "ok".to_string(),
                is_error: false,
                decision: Some(ApprovalDecision::Approved),
            }],
            encrypted: BTreeMap::new(),
        };
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(value["type"], "tool_results");
        assert_eq!(value["data"]["results"][0]["call_id"], "c1");
        assert_eq!(value["data"]["results"][0]["decision"], "approved");
        assert!(value["data"].get("encrypted").is_none());
    }

    #[test]
    fn should_carry_sealed_records_on_tool_results() {
        let mut encrypted = BTreeMap::new();
        encrypted.insert(ContentRef::Message("0194".to_string()), envelope());
        let msg = ContinueRequest::ToolResults {
            results: Vec::new(),
            encrypted,
        };
        let value = serde_json::to_value(&msg).unwrap();
        assert_eq!(
            value["data"]["encrypted"]["message:0194"]["key_id"],
            "kf_ab12"
        );
        assert_eq!(
            serde_json::from_value::<ContinueRequest>(value).unwrap(),
            msg
        );
    }

    #[test]
    fn should_serialize_break_as_bare_type() {
        let value = serde_json::to_value(ContinueRequest::Break).unwrap();
        assert_eq!(value, serde_json::json!({ "type": "break" }));
        assert_eq!(
            serde_json::from_value::<ContinueRequest>(value).unwrap(),
            ContinueRequest::Break
        );
    }

    #[test]
    fn should_roundtrip_decrypted_map() {
        let mut plaintext = BTreeMap::new();
        plaintext.insert(
            ContentRef::Message("0194".to_string()),
            "the body".to_string(),
        );
        let msg = ContinueRequest::Decrypted {
            plaintext,
            encrypted: BTreeMap::new(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(serde_json::from_str::<ContinueRequest>(&json).unwrap(), msg);
    }

    #[test]
    fn should_carry_encrypted_on_decrypted_continuation() {
        let json = r#"{"type":"decrypted","data":{"plaintext":{"message:1":"hi"},"encrypted":{}}}"#;
        let req: ContinueRequest = serde_json::from_str(json).unwrap();
        assert!(matches!(req, ContinueRequest::Decrypted { .. }));
    }

    #[test]
    fn should_roundtrip_inject_with_ciphertext() {
        let msg = ContinueRequest::Inject {
            messages: vec![UserMessage {
                text: "stop, do Y instead".to_string(),
                ciphertext: Some(envelope()),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(serde_json::from_str::<ContinueRequest>(&json).unwrap(), msg);
    }

    #[test]
    fn should_name_every_continue_kind_in_snake_case() {
        assert_eq!(
            serde_json::to_value(ContinueKind::ToolResults).unwrap(),
            "tool_results"
        );
        assert_eq!(
            serde_json::to_value(ContinueKind::Sealed).unwrap(),
            "sealed"
        );
        assert_eq!(serde_json::to_value(ContinueKind::Break).unwrap(), "break");
    }
}

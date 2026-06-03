//! Request and response bodies for submitting an approval decision.
//!
//! When a model-requested action needs user confirmation, the client submits a
//! decision to `POST /sessions/{id}/approvals/{approval_id}` with a
//! [`SubmitApprovalRequest`] and receives a [`SubmitApprovalResponse`] echoing
//! the recorded decision.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::{ApprovalDecision, SubmitApprovalRequest};
//!
//! let request = SubmitApprovalRequest {
//!     decision: ApprovalDecision::Approved,
//!     reason: None,
//! };
//! let json = serde_json::to_string(&request).unwrap();
//! assert!(json.contains("\"decision\":\"approved\""));
//! ```

use serde::{Deserialize, Serialize};
#[cfg(feature = "surrealdb")]
use surrealdb_types::SurrealValue;

/// The decision made on a pending approval.
///
/// Each variant serializes to its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "surrealdb", derive(SurrealValue))]
#[cfg_attr(
    feature = "surrealdb",
    surreal(crate = "::surrealdb_types", rename_all = "lowercase")
)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ApprovalDecision {
    /// The action is permitted to proceed.
    Approved,
    /// The action is denied.
    Rejected,
}

/// Body of `POST /sessions/{id}/approvals/{approval_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SubmitApprovalRequest {
    /// The decision being submitted.
    pub decision: ApprovalDecision,
    /// Optional human-readable reason for the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

/// Response confirming the recorded approval decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SubmitApprovalResponse {
    /// Identifier of the approval the decision applies to.
    pub approval_id: String,
    /// The recorded decision.
    pub decision: ApprovalDecision,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_decision_as_lowercase() {
        assert_eq!(
            serde_json::to_value(ApprovalDecision::Rejected).unwrap(),
            serde_json::json!("rejected")
        );
    }

    #[test]
    fn should_deserialize_spec_request() {
        let request: SubmitApprovalRequest =
            serde_json::from_str(r#"{"decision":"approved","reason":null}"#).unwrap();
        assert_eq!(request.decision, ApprovalDecision::Approved);
        assert_eq!(request.reason, None);
    }

    #[test]
    fn should_omit_absent_reason() {
        let request = SubmitApprovalRequest {
            decision: ApprovalDecision::Approved,
            reason: None,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({ "decision": "approved" })
        );
    }

    #[test]
    fn should_roundtrip_response() {
        let response = SubmitApprovalResponse {
            approval_id: "approval:abc".to_string(),
            decision: ApprovalDecision::Approved,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            serde_json::from_str::<SubmitApprovalResponse>(&json).unwrap(),
            response
        );
    }
}

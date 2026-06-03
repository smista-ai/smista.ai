//! Session approval table.

use chrono::{DateTime, Utc};
use smista_core::api::ApprovalDecision;
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

/// Records a user decision for an operation that required confirmation.
///
/// Covers a tool call, file write, shell command, network access or
/// remote-provider context disclosure. Metadata-only: there is no paired
/// content table.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionApproval {
    /// Unique identifier for the approval.
    pub id: RecordId,
    /// Session the approval belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Type of operation being approved.
    pub target_type: String,
    /// Id of the operation being approved.
    pub target_id: String,
    /// Approve or reject.
    pub decision: ApprovalDecision,
    /// Why the decision was made.
    pub reason: Option<String>,
    /// When the decision was recorded.
    pub created_at: DateTime<Utc>,
}

impl Table for SessionApproval {
    fn name() -> &'static str {
        "session_approval"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session_approval() {
        let id = RecordId::new(SessionApproval::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let approval = SessionApproval {
            id,
            session: session.clone(),
            user: user.clone(),
            target_type: "tool_call".to_string(),
            target_id: "session_tool_call:abc".to_string(),
            decision: ApprovalDecision::Approved,
            reason: Some("looks safe".to_string()),
            created_at: Utc::now(),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), approval).await;
    }
}

//! Session plan tables.
//!
//! A [`SessionPlan`] holds the queryable metadata of a generated or approved
//! execution plan; the plan snapshot lives in the paired
//! [`SessionPlanContent`] table under the same record id.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;
use crate::types::SecretContent;

/// Lifecycle status of a plan.
///
/// **Provisional**: these variants are a placeholder until the private spec
/// pins the plan lifecycle. Serialized as `snake_case`.
#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
#[surreal(rename_all = "snake_case", untagged)]
pub enum PlanStatus {
    /// Generated, not yet approved.
    Draft,
    /// Approved for execution.
    Approved,
    /// Rejected by the user.
    Rejected,
}

/// Records a generated or approved execution plan.
///
/// The plan snapshot lives in [`SessionPlanContent`]; the queryable hash and
/// status stay here.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionPlan {
    /// Unique identifier for the plan.
    pub id: RecordId,
    /// Session the plan belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Path the plan applies to.
    pub path: String,
    /// Plan status.
    pub status: PlanStatus,
    /// When the plan was created.
    pub created_at: DateTime<Utc>,
    /// When the plan was last updated.
    pub updated_at: DateTime<Utc>,
    /// When the plan was approved, if applicable.
    pub approved_at: Option<DateTime<Utc>>,
    /// Hash of the plan snapshot.
    pub content_hash: Option<String>,
}

impl Table for SessionPlan {
    fn name() -> &'static str {
        "session_plan"
    }
}

/// The snapshot of a [`SessionPlan`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionPlanContent {
    /// Record id, identical to the owning [`SessionPlan`].
    pub id: RecordId,
    /// Snapshot of the plan body, in clear or sealed for an encrypted session.
    pub content_snapshot: Option<SecretContent>,
}

impl Table for SessionPlanContent {
    fn name() -> &'static str {
        "session_plan_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session_plan() {
        let id = RecordId::new(SessionPlan::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let plan = SessionPlan {
            id,
            session: session.clone(),
            user: user.clone(),
            path: "src/lib.rs".to_string(),
            status: PlanStatus::Approved,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            approved_at: Some(Utc::now()),
            content_hash: Some("deadbeef".to_string()),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), plan).await;
    }

    #[tokio::test]
    async fn should_store_and_read_session_plan_content() {
        let id = RecordId::new(SessionPlanContent::name(), uuid::Uuid::now_v7().to_string());
        let content = SessionPlanContent {
            id,
            content_snapshot: Some(SecretContent::plaintext("1. do the thing")),
        };

        crate::tests::roundtrip(content).await;
    }
}

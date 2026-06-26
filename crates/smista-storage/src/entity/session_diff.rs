//! Session diff tables.
//!
//! A [`SessionDiff`] holds the queryable metadata of a proposed or applied file
//! modification; the diff body lives in the paired [`SessionDiffContent`] table
//! under the same record id and is stored only after secret filtering.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;
use crate::types::SecretContent;

/// Lifecycle status of a diff.
///
/// **Provisional**: these variants are a placeholder until the private spec
/// pins the diff lifecycle. Serialized as `snake_case`.
#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
#[surreal(rename_all = "snake_case", untagged)]
pub enum DiffStatus {
    /// Proposed, not yet applied.
    Proposed,
    /// Applied to the working tree.
    Applied,
    /// Rejected by the user.
    Rejected,
}

/// Records a proposed or applied file modification.
///
/// The diff body lives in [`SessionDiffContent`] and is stored only after
/// secret filtering, per the active privacy policy.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionDiff {
    /// Unique identifier for the diff.
    pub id: RecordId,
    /// Session the diff belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Path the diff applies to.
    pub path: String,
    /// Diff status.
    pub status: DiffStatus,
    /// When the diff was created.
    pub created_at: DateTime<Utc>,
    /// When the diff was applied, if applicable.
    pub applied_at: Option<DateTime<Utc>>,
}

impl Table for SessionDiff {
    fn name() -> &'static str {
        "session_diff"
    }
}

/// The body of a [`SessionDiff`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionDiffContent {
    /// Record id, identical to the owning [`SessionDiff`].
    pub id: RecordId,
    /// The diff body (secret-filtered), in clear or sealed for an encrypted session.
    pub content: SecretContent,
}

impl Table for SessionDiffContent {
    fn name() -> &'static str {
        "session_diff_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session_diff() {
        let id = RecordId::new(SessionDiff::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let diff = SessionDiff {
            id,
            session: session.clone(),
            user: user.clone(),
            path: "src/lib.rs".to_string(),
            status: DiffStatus::Applied,
            created_at: Utc::now(),
            applied_at: Some(Utc::now()),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), diff).await;
    }

    #[tokio::test]
    async fn should_store_and_read_session_diff_content() {
        let id = RecordId::new(SessionDiffContent::name(), uuid::Uuid::now_v7().to_string());
        let content = SessionDiffContent {
            id,
            content: SecretContent::plaintext("@@ -1 +1 @@\n-old\n+new"),
        };

        crate::tests::roundtrip(content).await;
    }
}

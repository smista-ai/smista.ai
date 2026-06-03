//! Session context reference table.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

/// Records what context was selected or excluded for a task.
///
/// Stores references and metadata only — not full file contents. Restricted
/// contents are not persisted unless policy allows. Metadata-only: there is no
/// paired content table.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionContextReference {
    /// Unique identifier for the reference.
    pub id: RecordId,
    /// Session the reference belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Path of the referenced context.
    pub path: Option<String>,
    /// Kind of context reference.
    pub kind: String,
    /// Whether the context was included.
    pub included: bool,
    /// Why it was included or excluded.
    pub reason: String,
    /// When the reference was recorded.
    pub created_at: DateTime<Utc>,
}

impl Table for SessionContextReference {
    fn name() -> &'static str {
        "session_context_reference"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn should_store_and_read_session_context_reference() {
        let id = RecordId::new(
            SessionContextReference::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let session = RecordId::new(
            crate::entity::Session::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let user = RecordId::new(
            crate::entity::User::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let reference = SessionContextReference {
            id,
            session: session.clone(),
            user: user.clone(),
            path: Some("src/main.rs".to_string()),
            kind: "file".to_string(),
            included: true,
            reason: "edited file".to_string(),
            created_at: Utc::now(),
        };

        crate::tests::fk_roundtrip(crate::tests::session(session, user), reference).await;
    }
}

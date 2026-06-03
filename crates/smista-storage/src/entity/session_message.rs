//! Session message tables.
//!
//! A [`SessionMessage`] holds the queryable metadata of a message exchanged
//! during a session; its body lives in the paired [`SessionMessageContent`]
//! table, which shares the same record id (`session_message:⟨uuid⟩` ↔
//! `session_message_content:⟨uuid⟩`). The split keeps metadata queryable while
//! the content may later be encrypted before persistence.

use chrono::{DateTime, Utc};
use smista_core::message::MessageRole;
use smista_core::model::Provider;
use surrealdb::types::{RecordId, SurrealValue};

use super::Table;

/// A message exchanged during a session.
///
/// Metadata stays queryable here; the message body lives in
/// [`SessionMessageContent`] under the same record id. Both `session` and
/// `user` are stored as explicit references — `user` redundantly, so ownership
/// checks never need a join.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionMessage {
    /// Unique identifier for the message.
    pub id: RecordId,
    /// Session the message belongs to.
    pub session: RecordId,
    /// Owner, enforced on every query.
    pub user: RecordId,
    /// Role of the message's author.
    pub role: MessageRole,
    /// Provider that produced the message.
    pub provider: Provider,
    /// Model that produced the message.
    pub model: String,
    /// When the message was recorded.
    pub created_at: DateTime<Utc>,
}

impl Table for SessionMessage {
    fn name() -> &'static str {
        "session_message"
    }
}

/// The body of a [`SessionMessage`], paired 1:1 by record id.
#[derive(Debug, Clone, SurrealValue, PartialEq, Eq)]
pub struct SessionMessageContent {
    /// Record id, identical to the owning [`SessionMessage`].
    pub id: RecordId,
    /// The message body.
    pub content: String,
}

impl Table for SessionMessageContent {
    fn name() -> &'static str {
        "session_message_content"
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn message(id: RecordId, session: RecordId, user: RecordId) -> SessionMessage {
        SessionMessage {
            id,
            session,
            user,
            role: MessageRole::Assistant,
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn should_store_and_read_session_message() {
        let id = RecordId::new(SessionMessage::name(), uuid::Uuid::now_v7().to_string());
        let session = RecordId::new("session", uuid::Uuid::now_v7().to_string());
        let user = RecordId::new("user", uuid::Uuid::now_v7().to_string());

        crate::tests::fk_roundtrip(crate::tests::user(user.clone()), message(id, session, user))
            .await;
    }

    #[tokio::test]
    async fn should_store_and_read_session_message_content() {
        let id = RecordId::new(
            SessionMessageContent::name(),
            uuid::Uuid::now_v7().to_string(),
        );
        let content = SessionMessageContent {
            id,
            content: "the message body".to_string(),
        };

        crate::tests::roundtrip(content).await;
    }
}

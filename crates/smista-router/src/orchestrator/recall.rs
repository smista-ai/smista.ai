//! Recall of session history and memory for a turn, gated on decryption.
//!
//! The resolver needs a [`RecalledContext`] built from stored history and
//! memory. For an encrypted session the stored content is sealed and the router
//! holds no key, so recall cannot proceed until the client opens the rows it
//! needs. [`recall`] reads the rows and, for each sealed payload, uses the
//! supplied plaintext when present or collects the row's [`ContentRef`] into a
//! decrypt request otherwise. It returns [`Recalled::Ready`] only when every
//! needed row is available in plaintext.
#![allow(
    dead_code,
    reason = "wired into the turn loop in a later orchestrator task"
)]

use std::collections::BTreeMap;

use smista_core::api::ContentRef;
use smista_storage::types::SecretContent;
use uuid::Uuid;

use crate::orchestrator::crypto::record_id_to_content_ref;
use crate::router::resolver::context::{MemoryFact, RecalledContext, RecalledMessage};
use crate::session::{SessionResult, UserSession};

/// The outcome of recalling a session's context.
pub(crate) enum Recalled {
    /// Every needed row was available in plaintext; the context is ready.
    Ready(RecalledContext),
    /// One or more sealed rows must be opened before the context can be built.
    NeedsDecrypt(Vec<ContentRef>),
}

/// Builds the [`RecalledContext`] for a turn from stored history and memory.
///
/// `decrypted` carries the plaintext the client has already opened, keyed by
/// [`ContentRef`]. A sealed row whose plaintext is present is used directly; a
/// sealed row that is still absent is collected into the returned
/// [`Recalled::NeedsDecrypt`]. User memory is user-scoped and never encrypted,
/// so it is always used directly.
///
/// # Errors
///
/// Returns [`SessionError`](crate::session::SessionError) if any row cannot be
/// read.
pub(crate) async fn recall(
    session: &UserSession,
    decrypted: &BTreeMap<ContentRef, String>,
) -> SessionResult<Recalled> {
    let mut needs = Vec::new();

    let mut messages = Vec::new();
    for (meta, content) in session.messages().await? {
        if let Some(text) = resolve_secret(
            &content.content,
            "session_message",
            meta.uuid(),
            decrypted,
            &mut needs,
        ) {
            messages.push(RecalledMessage {
                role: meta.role,
                path: None,
                content: text,
            });
        }
    }

    let mut context_memory = Vec::new();
    for (meta, content) in session.list_context_memory_with_content().await? {
        if let Some(text) = resolve_secret(
            &content.content,
            "context_memory",
            meta.uuid(),
            decrypted,
            &mut needs,
        ) {
            context_memory.push(MemoryFact {
                key: meta.key.clone().unwrap_or_default(),
                content: text,
            });
        }
    }

    let user_memory = session
        .list_user_memory_with_content()
        .await?
        .into_iter()
        .map(|(meta, content)| MemoryFact {
            key: meta.key.clone().unwrap_or_default(),
            content: content.content,
        })
        .collect();

    if !needs.is_empty() {
        tracing::debug!(sealed = needs.len(), "recall requires decrypt");
        return Ok(Recalled::NeedsDecrypt(needs));
    }

    Ok(Recalled::Ready(RecalledContext {
        messages,
        user_memory,
        context_memory,
    }))
}

/// Resolves one sealable payload to plaintext, or records it as needing decrypt.
///
/// Returns the plaintext when the row is stored in clear or its opened text is
/// in `decrypted`; otherwise pushes the row's [`ContentRef`] onto `needs` and
/// returns `None`.
fn resolve_secret(
    content: &SecretContent,
    table: &str,
    id: Uuid,
    decrypted: &BTreeMap<ContentRef, String>,
    needs: &mut Vec<ContentRef>,
) -> Option<String> {
    if let Some(text) = content.as_plaintext() {
        return Some(text.to_string());
    }
    let reference = record_id_to_content_ref(table, id)?;
    match decrypted.get(&reference) {
        Some(text) => Some(text.clone()),
        None => {
            needs.push(reference);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use smista_core::message::MessageRole;
    use smista_core::model::Provider;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use smista_storage::entity::{Session, SessionMessage, SessionMessageContent, Table, User};
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::ContentEnvelope;

    use super::*;
    use crate::session::Sessions;

    async fn memory_db() -> SurrealDatabase {
        SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database")
    }

    fn user_entity(id: Uuid) -> User {
        User {
            id: RecordId::new(User::name(), id.to_string()),
            api_key_hash: format!("hash-{id}"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        }
    }

    fn session_entity(id: Uuid, user_id: Uuid, encrypted: bool) -> Session {
        Session {
            id: RecordId::new(Session::name(), id.to_string()),
            user: RecordId::new(User::name(), user_id.to_string()),
            title: Some("session".to_string()),
            encrypted,
            key_id: encrypted.then(|| "kf_test".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            archived_at: None,
        }
    }

    fn message(
        id: Uuid,
        session_id: Uuid,
        user_id: Uuid,
        content: SecretContent,
    ) -> (SessionMessage, SessionMessageContent) {
        (
            SessionMessage {
                id: RecordId::new(SessionMessage::name(), id.to_string()),
                session: RecordId::new(Session::name(), session_id.to_string()),
                user: RecordId::new(User::name(), user_id.to_string()),
                role: MessageRole::User,
                provider: Provider::Anthropic,
                model: "claude".to_string(),
                created_at: Utc::now(),
            },
            SessionMessageContent {
                id: RecordId::new(SessionMessageContent::name(), id.to_string()),
                content,
            },
        )
    }

    fn envelope() -> ContentEnvelope {
        ContentEnvelope {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_test".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    async fn open(encrypted: bool) -> (UserSession, Uuid, Uuid) {
        let db = memory_db().await;
        let user_id = Uuid::now_v7();
        db.create_user(user_entity(user_id))
            .await
            .expect("failed to create user");
        let sessions = Sessions::new(db, user_id);
        let session_id = Uuid::now_v7();
        let handle = sessions
            .create(session_entity(session_id, user_id, encrypted))
            .await
            .expect("failed to create session");
        (handle, user_id, session_id)
    }

    #[tokio::test]
    async fn should_recall_plaintext_history_directly() {
        let (session, user_id, session_id) = open(false).await;
        for text in ["first", "second"] {
            let (meta, content) = message(
                Uuid::now_v7(),
                session_id,
                user_id,
                SecretContent::plaintext(text),
            );
            session
                .append_message(meta, content)
                .await
                .expect("failed to append message");
        }

        match recall(&session, &BTreeMap::new()).await.unwrap() {
            Recalled::Ready(ctx) => assert_eq!(ctx.messages.len(), 2),
            Recalled::NeedsDecrypt(_) => panic!("expected ready"),
        }
    }

    #[tokio::test]
    async fn should_request_decrypt_for_sealed_history() {
        let (session, user_id, session_id) = open(true).await;
        let (meta, content) = message(
            Uuid::now_v7(),
            session_id,
            user_id,
            SecretContent::from(envelope()),
        );
        session
            .append_message(meta, content)
            .await
            .expect("failed to append message");

        match recall(&session, &BTreeMap::new()).await.unwrap() {
            Recalled::NeedsDecrypt(refs) => assert_eq!(refs.len(), 1),
            Recalled::Ready(_) => panic!("expected needs-decrypt"),
        }
    }

    #[tokio::test]
    async fn should_use_supplied_plaintext_for_sealed_history() {
        let (session, user_id, session_id) = open(true).await;
        let id = Uuid::now_v7();
        let (meta, content) = message(id, session_id, user_id, SecretContent::from(envelope()));
        session
            .append_message(meta, content)
            .await
            .expect("failed to append message");

        let mut decrypted = BTreeMap::new();
        decrypted.insert(
            ContentRef::Message(id.to_string()),
            "opened text".to_string(),
        );

        match recall(&session, &decrypted).await.unwrap() {
            Recalled::Ready(ctx) => assert_eq!(ctx.messages[0].content, "opened text"),
            Recalled::NeedsDecrypt(_) => panic!("expected ready"),
        }
    }
}

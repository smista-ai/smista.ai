use chrono::{Duration, Utc};
use smista_core::intent::TaskIntent;
use smista_core::message::MessageRole;
use smista_core::model::Provider;
use smista_core::trace::{Payload, RoutingDecisionPayload, TraceEventPayload};
use surrealdb::types::RecordId;

use super::*;
use crate::api::Pagination;
use crate::entity::{ToolCallStatus, TraceEventType};
use crate::types::{ContentEnvelope, SecretContent};

async fn memory_db() -> SurrealDatabase {
    SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Memory,
    })
    .await
    .expect("failed to initialize database")
}

fn user(id: Uuid) -> User {
    User {
        id: RecordId::new(User::name(), id.to_string()),
        api_key_hash: format!("hash-{id}"),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        disabled_at: None,
    }
}

fn auth_token(id: Uuid, user_id: Uuid, hash: &str) -> AuthToken {
    AuthToken {
        id: RecordId::new(AuthToken::name(), id.to_string()),
        user: RecordId::new(User::name(), user_id.to_string()),
        token_hash: hash.to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now(),
        revoked_at: None,
    }
}

#[tokio::test]
async fn new_applies_schema_and_is_idempotent() {
    // The first connect applies the migration to an empty database.
    let db = memory_db().await;

    // Re-running the migration on the same connection is a no-op, proving
    // every `DEFINE ... IF NOT EXISTS` statement is idempotent.
    schema::apply(&db.0)
        .await
        .expect("schema migration is not idempotent");
}

#[tokio::test]
async fn embedded_surrealkv_creates_dir_and_applies_schema() {
    let root = tempfile::tempdir().expect("failed to create temp dir");
    // A nested, not-yet-existing directory proves `prepare` creates it.
    let db_dir = root.path().join("surrealkv");

    // Connecting opens the SurrealKV store on disk and applies the schema.
    let db = SurrealDatabase::new(SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend: SurrealBackend::Embedded {
            db_dir: db_dir.clone(),
        },
    })
    .await
    .expect("failed to open embedded database");

    assert!(db_dir.is_dir(), "embedded data directory was not created");

    // Re-applying the migration confirms it landed on the persisted store.
    schema::apply(&db.0)
        .await
        .expect("schema migration is not idempotent");
}

#[tokio::test]
async fn should_create_user() {
    let db = memory_db().await;

    let id = Uuid::now_v7();
    let user = user(id);

    let created = db
        .create_user(user.clone())
        .await
        .expect("failed to create user");

    assert_eq!(created, user);
}

#[tokio::test]
async fn should_not_create_duplicate_user() {
    let db = memory_db().await;

    let id = Uuid::now_v7();
    let user = user(id);

    db.create_user(user.clone())
        .await
        .expect("failed to create user");

    let err = db
        .create_user(user)
        .await
        .expect_err("creating duplicate user did not fail");

    assert!(matches!(err, StorageError::Backend(_)));
}

#[tokio::test]
async fn should_get_user() {
    let db = memory_db().await;

    let id = Uuid::now_v7();
    let user = user(id);

    assert!(
        db.get_user(id).await.expect("failed to get user").is_none(),
        "unexpectedly found user before creation"
    );

    db.create_user(user.clone())
        .await
        .expect("failed to create user");

    let got = db
        .get_user(id)
        .await
        .expect("failed to get user")
        .expect("user not found after creation");

    assert_eq!(got, user);
}

#[tokio::test]
async fn should_not_get_nonexistent_user() {
    let db = memory_db().await;

    let id = Uuid::now_v7();

    assert!(
        db.get_user(id).await.expect("failed to get user").is_none(),
        "unexpectedly found user that was never created"
    );
}

#[tokio::test]
async fn should_create_auth_token() {
    let db = memory_db().await;

    let user_id = Uuid::now_v7();
    let user = user(user_id);

    // insert user
    db.create_user(user.clone())
        .await
        .expect("failed to create user");

    let token_id = Uuid::now_v7();
    let token = auth_token(token_id, user_id, "hash");

    let created = db
        .create_token(token.clone())
        .await
        .expect("failed to create auth token");

    assert_eq!(created, token);
}

#[tokio::test]
async fn should_not_create_auth_token_if_user_does_not_exist() {
    let db = memory_db().await;

    let user_id = Uuid::now_v7();

    let token_id = Uuid::now_v7();
    let token = auth_token(token_id, user_id, "hash");

    let err = db
        .create_token(token)
        .await
        .expect_err("creating auth token for non-existent user did not fail");

    assert!(matches!(err, StorageError::Backend(_)));
}

#[tokio::test]
async fn should_not_create_duplicate_auth_token() {
    let db = memory_db().await;

    let user_id = Uuid::now_v7();
    let user = user(user_id);

    // insert user
    db.create_user(user.clone())
        .await
        .expect("failed to create user");

    let token_id = Uuid::now_v7();
    let token = auth_token(token_id, user_id, "hash");

    db.create_token(token.clone())
        .await
        .expect("failed to create auth token");

    let err = db
        .create_token(token)
        .await
        .expect_err("creating duplicate auth token did not fail");

    assert!(matches!(err, StorageError::Backend(_)));
}

#[tokio::test]
async fn should_create_multiple_auth_tokens_for_same_user() {
    let db = memory_db().await;

    let user_id = Uuid::now_v7();
    let user = user(user_id);

    // insert user
    db.create_user(user.clone())
        .await
        .expect("failed to create user");

    let token1 = auth_token(Uuid::now_v7(), user_id, "hash1");
    let token2 = auth_token(Uuid::now_v7(), user_id, "hash2");

    let created1 = db
        .create_token(token1.clone())
        .await
        .expect("failed to create first auth token");
    let created2 = db
        .create_token(token2.clone())
        .await
        .expect("failed to create second auth token");

    assert_eq!(created1, token1);
    assert_eq!(created2, token2);
}

// -- Builders ---------------------------------------------------------------

fn token_for(
    id: Uuid,
    user_id: Uuid,
    hash: &str,
    expires_at: chrono::DateTime<Utc>,
    revoked_at: Option<chrono::DateTime<Utc>>,
) -> AuthToken {
    AuthToken {
        id: record_id::<AuthToken, _>(id),
        user: record_id::<User, _>(user_id),
        token_hash: hash.to_string(),
        created_at: Utc::now(),
        expires_at,
        revoked_at,
    }
}

fn session_for(id: Uuid, user_id: Uuid) -> Session {
    Session {
        id: record_id::<Session, _>(id),
        user: record_id::<User, _>(user_id),
        title: Some("session".to_string()),
        encrypted: false,
        key_id: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        archived_at: None,
    }
}

fn message_for(
    id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
) -> (SessionMessage, SessionMessageContent) {
    (
        SessionMessage {
            id: record_id::<SessionMessage, _>(id),
            session: record_id::<Session, _>(session_id),
            user: record_id::<User, _>(user_id),
            role: MessageRole::User,
            provider: Provider::Anthropic,
            model: "claude".to_string(),
            created_at: Utc::now(),
        },
        SessionMessageContent {
            id: record_id::<SessionMessageContent, _>(id),
            content: SecretContent::plaintext("hello"),
        },
    )
}

fn tool_call_for(
    id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
) -> (SessionToolCall, SessionToolCallContent) {
    (
        SessionToolCall {
            id: record_id::<SessionToolCall, _>(id),
            session: record_id::<Session, _>(session_id),
            user: record_id::<User, _>(user_id),
            tool_name: "read_file".to_string(),
            status: ToolCallStatus::Completed,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
        },
        SessionToolCallContent {
            id: record_id::<SessionToolCallContent, _>(id),
            arguments: SecretContent::plaintext("{}"),
            result: Some(SecretContent::plaintext("ok")),
            error: None,
        },
    )
}

fn routing_decision_for(id: Uuid, session_id: Uuid, user_id: Uuid) -> SessionRoutingDecision {
    SessionRoutingDecision {
        id: record_id::<SessionRoutingDecision, _>(id),
        session: record_id::<Session, _>(session_id),
        user: record_id::<User, _>(user_id),
        task_type: TaskIntent::Edit,
        provider: Provider::OpenAI,
        model: "gpt".to_string(),
        matched_rule: None,
        fallback_used: None,
        override_used: None,
        reason: "best".to_string(),
        created_at: Utc::now(),
    }
}

fn trace_event_for(
    id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
    provider: Provider,
    model: &str,
    created_at: chrono::DateTime<Utc>,
) -> (TraceEvent, TraceEventContent) {
    let payload = Payload::RoutingDecision(RoutingDecisionPayload {
        provider: provider.clone(),
        model: model.to_string(),
        matched_rule: Some("edit -> model".to_string()),
        fallback_used: false,
        override_used: false,
        reason: "test".to_string(),
    });
    (
        TraceEvent {
            id: record_id::<TraceEvent, _>(id),
            session: record_id::<Session, _>(session_id),
            user: record_id::<User, _>(user_id),
            event_type: TraceEventType::RoutingDecision,
            task_type: TaskIntent::Edit,
            provider,
            model: model.to_string(),
            matched_rule: Some("edit -> model".to_string()),
            created_at,
        },
        TraceEventContent {
            id: record_id::<TraceEventContent, _>(id),
            payload: SecretContent::plaintext(
                serde_json::to_string(&payload).expect("failed to serialize trace payload"),
            ),
        },
    )
}

fn user_memory_for(
    id: Uuid,
    user_id: Uuid,
    key: Option<&str>,
    content: &str,
) -> (UserMemory, UserMemoryContent) {
    (
        UserMemory {
            id: record_id::<UserMemory, _>(id),
            user: record_id::<User, _>(user_id),
            key: key.map(str::to_string),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        UserMemoryContent {
            id: record_id::<UserMemoryContent, _>(id),
            content: content.to_string(),
        },
    )
}

fn context_memory_for(
    id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
    key: Option<&str>,
    content: &str,
) -> (ContextMemory, ContextMemoryContent) {
    (
        ContextMemory {
            id: record_id::<ContextMemory, _>(id),
            session: record_id::<Session, _>(session_id),
            user: record_id::<User, _>(user_id),
            key: key.map(str::to_string),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        ContextMemoryContent {
            id: record_id::<ContextMemoryContent, _>(id),
            content: SecretContent::plaintext(content),
        },
    )
}

/// Counts every row currently stored in table `T`.
async fn count<T>(db: &SurrealDatabase) -> usize
where
    T: SurrealValue + Table + Send + 'static,
{
    let rows: Vec<T> =
        db.0.select(T::name())
            .await
            .expect("failed to select table");
    rows.len()
}

/// Creates a user and an owned session, returning their ids.
async fn user_with_session(db: &SurrealDatabase) -> (Uuid, Uuid) {
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let session_id = Uuid::now_v7();
    db.create_session(session_for(session_id, user_id))
        .await
        .expect("failed to create session");

    (user_id, session_id)
}

// -- Tokens -----------------------------------------------------------------

#[tokio::test]
async fn should_not_create_token_with_duplicate_hash() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    db.create_token(token_for(
        Uuid::now_v7(),
        user_id,
        "same",
        Utc::now() + Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create first token");

    let err = db
        .create_token(token_for(
            Uuid::now_v7(),
            user_id,
            "same",
            Utc::now() + Duration::hours(1),
            None,
        ))
        .await
        .expect_err("duplicate token hash was accepted");

    assert!(matches!(err, StorageError::Backend(_)));
}

#[tokio::test]
async fn should_get_active_token() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");
    let id = Uuid::now_v7();
    db.create_token(token_for(
        id,
        user_id,
        "live",
        Utc::now() + Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create token");

    let token = db
        .get_active_token(id)
        .await
        .expect("failed to get token")
        .expect("active token not found");

    assert_eq!(token.token_hash, "live");
    // The owning user is recoverable from the loaded token.
    assert_eq!(token.user_id(), user_id);
}

#[tokio::test]
async fn should_not_get_expired_or_revoked_token() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");
    let expired_id = Uuid::now_v7();
    db.create_token(token_for(
        expired_id,
        user_id,
        "expired",
        Utc::now() - Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create expired token");
    let revoked_id = Uuid::now_v7();
    db.create_token(token_for(
        revoked_id,
        user_id,
        "revoked",
        Utc::now() + Duration::hours(1),
        Some(Utc::now()),
    ))
    .await
    .expect("failed to create revoked token");

    assert!(
        db.get_active_token(expired_id)
            .await
            .expect("failed to get token")
            .is_none(),
        "expired token returned"
    );
    assert!(
        db.get_active_token(revoked_id)
            .await
            .expect("failed to get token")
            .is_none(),
        "revoked token returned"
    );
}

#[tokio::test]
async fn should_get_token_regardless_of_expiry_or_revocation() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");
    let expired_id = Uuid::now_v7();
    db.create_token(token_for(
        expired_id,
        user_id,
        "expired",
        Utc::now() - Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create expired token");
    let revoked_id = Uuid::now_v7();
    db.create_token(token_for(
        revoked_id,
        user_id,
        "revoked",
        Utc::now() + Duration::hours(1),
        Some(Utc::now()),
    ))
    .await
    .expect("failed to create revoked token");

    // Unlike `get_active_token`, `get_token` returns the row whatever its state,
    // so the auth layer can tell an expired or revoked token apart from one that
    // was never issued.
    let expired = db
        .get_token(expired_id)
        .await
        .expect("failed to get token")
        .expect("expired token not found");
    assert!(expired.expires_at < Utc::now());
    let revoked = db
        .get_token(revoked_id)
        .await
        .expect("failed to get token")
        .expect("revoked token not found");
    assert!(revoked.revoked_at.is_some());

    // An unknown id still yields nothing.
    assert!(
        db.get_token(Uuid::now_v7())
            .await
            .expect("failed to get token")
            .is_none(),
        "unknown token returned"
    );
}

#[tokio::test]
async fn should_revoke_token() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");
    let token_id = Uuid::now_v7();
    db.create_token(token_for(
        token_id,
        user_id,
        "to-revoke",
        Utc::now() + Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create token");

    db.revoke_token(token_id)
        .await
        .expect("failed to revoke token");

    assert!(
        db.get_active_token(token_id)
            .await
            .expect("failed to get token")
            .is_none(),
        "revoked token still valid"
    );
}

#[tokio::test]
async fn should_not_revoke_unknown_token() {
    let db = memory_db().await;
    let err = db
        .revoke_token(Uuid::now_v7())
        .await
        .expect_err("revoking unknown token did not fail");
    assert!(matches!(err, StorageError::NotFound));
}

// -- Sessions ---------------------------------------------------------------

#[tokio::test]
async fn should_create_and_get_session() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let got = db
        .get_session(user_id, session_id)
        .await
        .expect("failed to get session")
        .expect("session not found");

    assert_eq!(got.id, record_id::<Session, _>(session_id));
}

#[tokio::test]
async fn should_not_get_session_of_other_user() {
    let db = memory_db().await;
    let (_owner, session_id) = user_with_session(&db).await;

    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    assert!(
        db.get_session(other, session_id)
            .await
            .expect("failed to get session")
            .is_none(),
        "session of another user was disclosed"
    );
}

#[tokio::test]
async fn should_list_only_owned_sessions() {
    let db = memory_db().await;
    let (owner, first) = user_with_session(&db).await;
    db.create_session(session_for(Uuid::now_v7(), owner))
        .await
        .expect("failed to create second session");

    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");
    db.create_session(session_for(Uuid::now_v7(), other))
        .await
        .expect("failed to create foreign session");

    let owned = db
        .list_sessions(owner)
        .await
        .expect("failed to list sessions");

    assert_eq!(owned.len(), 2);
    assert!(owned.iter().all(|s| s.user == record_id::<User, _>(owner)));
    let _ = first;
}

#[tokio::test]
async fn should_archive_session() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    db.archive_session(user_id, session_id)
        .await
        .expect("failed to archive session");

    let got = db
        .get_session(user_id, session_id)
        .await
        .expect("failed to get session")
        .expect("session not found");
    assert!(got.archived_at.is_some());
}

#[tokio::test]
async fn should_not_archive_session_of_other_user() {
    let db = memory_db().await;
    let (_owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let err = db
        .archive_session(other, session_id)
        .await
        .expect_err("archived a session not owned");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_delete_session_and_cascade_children() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (message, message_content) = message_for(Uuid::now_v7(), session_id, user_id);
    db.append_message(user_id, message, message_content)
        .await
        .expect("failed to append message");

    let (tool_call, tool_content) = tool_call_for(Uuid::now_v7(), session_id, user_id);
    db.append_tool_call(user_id, tool_call, tool_content)
        .await
        .expect("failed to append tool call");

    db.append_routing_decision(
        user_id,
        routing_decision_for(Uuid::now_v7(), session_id, user_id),
    )
    .await
    .expect("failed to append routing decision");

    let (event, event_content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        user_id,
        Provider::Anthropic,
        "claude",
        Utc::now(),
    );
    db.append_trace_event(user_id, event, event_content)
        .await
        .expect("failed to append trace event");

    let (memory, memory_content) =
        context_memory_for(Uuid::now_v7(), session_id, user_id, None, "fact");
    db.record_context_memory(user_id, memory, memory_content)
        .await
        .expect("failed to record context memory");

    // A user memory must survive the cascade — it is not session-scoped.
    let (user_mem, user_mem_content) =
        user_memory_for(Uuid::now_v7(), user_id, Some("topic"), "kept");
    db.record_user_memory(user_id, user_mem, user_mem_content)
        .await
        .expect("failed to record user memory");

    db.delete_session(user_id, session_id)
        .await
        .expect("failed to delete session");

    assert!(
        db.get_session(user_id, session_id)
            .await
            .expect("failed to get session")
            .is_none(),
        "session not deleted"
    );
    assert_eq!(count::<SessionMessage>(&db).await, 0, "messages remain");
    assert_eq!(
        count::<SessionMessageContent>(&db).await,
        0,
        "message content remains"
    );
    assert_eq!(count::<SessionToolCall>(&db).await, 0, "tool calls remain");
    assert_eq!(
        count::<SessionToolCallContent>(&db).await,
        0,
        "tool call content remains"
    );
    assert_eq!(
        count::<SessionRoutingDecision>(&db).await,
        0,
        "routing decisions remain"
    );
    assert_eq!(count::<TraceEvent>(&db).await, 0, "trace events remain");
    assert_eq!(
        count::<TraceEventContent>(&db).await,
        0,
        "trace event content remains"
    );
    assert_eq!(
        count::<ContextMemory>(&db).await,
        0,
        "context memory remains"
    );
    assert_eq!(
        count::<ContextMemoryContent>(&db).await,
        0,
        "context memory content remains"
    );
    assert_eq!(
        count::<UserMemory>(&db).await,
        1,
        "user memory was wrongly cascaded"
    );
}

// -- Appends and reads ------------------------------------------------------

#[tokio::test]
async fn should_append_message_and_read_session_state() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (message, content) = message_for(Uuid::now_v7(), session_id, user_id);
    db.append_message(user_id, message.clone(), content)
        .await
        .expect("failed to append message");

    let state = db
        .get_session_state(user_id, session_id)
        .await
        .expect("failed to load session state")
        .expect("session state not found");

    assert_eq!(state.messages.len(), 1);
    assert_eq!(state.messages[0], message);
}

#[tokio::test]
async fn should_create_and_read_encrypted_session() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let session_id = Uuid::now_v7();
    let mut session = session_for(session_id, user_id);
    session.encrypted = true;
    session.key_id = Some("kf_ab12".to_string());
    db.create_session(session)
        .await
        .expect("failed to create encrypted session");

    let read = db
        .get_session(user_id, session_id)
        .await
        .expect("failed to load session")
        .expect("session not found");
    assert!(read.encrypted);
    assert_eq!(read.key_id.as_deref(), Some("kf_ab12"));
}

#[tokio::test]
async fn should_append_and_read_encrypted_message_content() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let id = Uuid::now_v7();
    let (message, _) = message_for(id, session_id, user_id);
    let sealed = ContentEnvelope {
        version: 1,
        algorithm: "xchacha20poly1305".to_string(),
        key_id: "kf_ab12".to_string(),
        nonce: "bm9uY2U".to_string(),
        ciphertext: "Y2lwaGVydGV4dA".to_string(),
    };
    let content = SessionMessageContent {
        id: record_id::<SessionMessageContent, _>(id),
        content: SecretContent::Encrypted(sealed.clone()),
    };
    db.append_message(user_id, message, content)
        .await
        .expect("failed to append encrypted message");

    // The encrypted envelope must round-trip through the SCHEMAFULL content
    // field, which storage holds opaquely.
    let rows: Vec<SessionMessageContent> =
        db.0.select(SessionMessageContent::name())
            .await
            .expect("failed to read content");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content, SecretContent::Encrypted(sealed));
}

#[tokio::test]
async fn should_not_append_message_to_unowned_session() {
    let db = memory_db().await;
    let (_owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let (message, content) = message_for(Uuid::now_v7(), session_id, other);
    let err = db
        .append_message(other, message, content)
        .await
        .expect_err("appended to a session not owned");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_not_append_message_with_forged_user() {
    let db = memory_db().await;
    let (owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    // The session belongs to `owner`, but the row claims `other` as its user.
    let (message, content) = message_for(Uuid::now_v7(), session_id, other);
    let err = db
        .append_message(owner, message, content)
        .await
        .expect_err("appended a row whose user was forged");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_not_append_routing_decision_with_forged_user() {
    let db = memory_db().await;
    let (owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let decision = routing_decision_for(Uuid::now_v7(), session_id, other);
    let err = db
        .append_routing_decision(owner, decision)
        .await
        .expect_err("appended a row whose user was forged");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_not_append_trace_event_with_forged_user() {
    let db = memory_db().await;
    let (owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let (event, content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        other,
        Provider::OpenAI,
        "gpt",
        Utc::now(),
    );
    let err = db
        .append_trace_event(owner, event, content)
        .await
        .expect_err("appended a row whose user was forged");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_not_record_context_memory_with_forged_user() {
    let db = memory_db().await;
    let (owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let (memory, content) = context_memory_for(Uuid::now_v7(), session_id, other, None, "fact");
    let err = db
        .record_context_memory(owner, memory, content)
        .await
        .expect_err("recorded a row whose user was forged");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_not_get_session_state_of_other_user() {
    let db = memory_db().await;
    let (_owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    assert!(
        db.get_session_state(other, session_id)
            .await
            .expect("failed to load session state")
            .is_none(),
        "session state of another user disclosed"
    );
}

#[tokio::test]
async fn should_assemble_trace_from_session_events() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (first, first_content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        user_id,
        Provider::OpenAI,
        "gpt",
        Utc::now() - Duration::minutes(5),
    );
    db.append_trace_event(user_id, first, first_content)
        .await
        .expect("failed to append first event");

    let (second, second_content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        user_id,
        Provider::Anthropic,
        "claude",
        Utc::now(),
    );
    db.append_trace_event(user_id, second, second_content)
        .await
        .expect("failed to append second event");

    let trace = db
        .get_session_trace_events(user_id, session_id, Pagination::default())
        .await
        .expect("failed to load trace")
        .expect("trace not found");

    assert_eq!(trace.session_id, session_id);
    assert_eq!(trace.events.len(), 2);
    // Events are ordered oldest first; each carries its own routing context.
    assert_eq!(trace.events[0].provider, Provider::OpenAI);
    assert_eq!(trace.events[0].model, "gpt");
    assert_eq!(trace.events[1].provider, Provider::Anthropic);
    assert_eq!(trace.events[1].model, "claude");
    // The plaintext payload reads back as its typed variant.
    assert!(matches!(
        trace.events[0].payload,
        TraceEventPayload::Plaintext(Payload::RoutingDecision(_))
    ));
}

#[tokio::test]
async fn should_paginate_session_trace_events() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    // Three events, one minute apart, so the oldest-first order is unambiguous.
    // Each carries a distinct model so the page contents can be asserted.
    for step in 0..3u32 {
        let (event, content) = trace_event_for(
            Uuid::now_v7(),
            session_id,
            user_id,
            Provider::OpenAI,
            &format!("gpt-{step}"),
            Utc::now() - Duration::minutes(i64::from(3 - step)),
        );
        db.append_trace_event(user_id, event, content)
            .await
            .expect("failed to append event");
    }

    // First page: the two oldest events.
    let first = db
        .get_session_trace_events(
            user_id,
            session_id,
            Pagination {
                limit: 2,
                offset: 0,
            },
        )
        .await
        .expect("failed to load first page")
        .expect("trace not found");
    assert_eq!(first.events.len(), 2);
    assert_eq!(first.events[0].model, "gpt-0");
    assert_eq!(first.events[1].model, "gpt-1");

    // Second page: the remaining event.
    let second = db
        .get_session_trace_events(
            user_id,
            session_id,
            Pagination {
                limit: 2,
                offset: 2,
            },
        )
        .await
        .expect("failed to load second page")
        .expect("trace not found");
    assert_eq!(second.events.len(), 1);
    assert_eq!(second.events[0].model, "gpt-2");

    // A page past the end resolves to an empty trace, not a missing one.
    let empty = db
        .get_session_trace_events(
            user_id,
            session_id,
            Pagination {
                limit: 2,
                offset: 10,
            },
        )
        .await
        .expect("failed to load empty page")
        .expect("trace not found");
    assert!(empty.events.is_empty());
}

#[tokio::test]
async fn should_return_empty_trace_without_events() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    // A session with no events resolves to an empty trace; `None` is reserved
    // for a session that does not exist or is not owned by the caller.
    let trace = db
        .get_session_trace_events(user_id, session_id, Pagination::default())
        .await
        .expect("failed to load trace")
        .expect("trace not found");
    assert_eq!(trace.session_id, session_id);
    assert!(trace.events.is_empty());
}

#[tokio::test]
async fn should_return_no_trace_for_unowned_session() {
    let db = memory_db().await;
    let (_owner, session_id) = user_with_session(&db).await;

    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    assert!(
        db.get_session_trace_events(other, session_id, Pagination::default())
            .await
            .expect("failed to load trace")
            .is_none(),
        "trace disclosed for a session owned by another user"
    );
}

// -- Memory -----------------------------------------------------------------

#[tokio::test]
async fn should_record_and_get_user_memory() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let id = Uuid::now_v7();
    let (memory, content) = user_memory_for(id, user_id, Some("editor"), "vim");
    db.record_user_memory(user_id, memory.clone(), content)
        .await
        .expect("failed to record memory");

    let got = db
        .get_user_memory(user_id, id)
        .await
        .expect("failed to get memory")
        .expect("memory not found");
    assert_eq!(got, memory);
}

#[tokio::test]
async fn should_upsert_keyed_user_memory_in_place() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let (first, first_content) = user_memory_for(Uuid::now_v7(), user_id, Some("editor"), "vim");
    db.record_user_memory(user_id, first, first_content)
        .await
        .expect("failed to record first memory");

    let (second, second_content) =
        user_memory_for(Uuid::now_v7(), user_id, Some("editor"), "emacs");
    db.record_user_memory(user_id, second, second_content)
        .await
        .expect("failed to record second memory");

    // The keyed memory was replaced in place: a single row for the key.
    let all = db
        .list_user_memory(user_id)
        .await
        .expect("failed to list memory");
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn should_keep_multiple_keyless_user_memories() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let (first, first_content) = user_memory_for(Uuid::now_v7(), user_id, None, "a");
    db.record_user_memory(user_id, first, first_content)
        .await
        .expect("failed to record first memory");
    let (second, second_content) = user_memory_for(Uuid::now_v7(), user_id, None, "b");
    db.record_user_memory(user_id, second, second_content)
        .await
        .expect("failed to record second memory");

    let all = db
        .list_user_memory(user_id)
        .await
        .expect("failed to list memory");
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn should_update_user_memory_by_key() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let id = Uuid::now_v7();
    let (memory, content) = user_memory_for(id, user_id, Some("editor"), "vim");
    db.record_user_memory(user_id, memory, content)
        .await
        .expect("failed to record memory");

    let new_content = UserMemoryContent {
        id: record_id::<UserMemoryContent, _>(id),
        content: "emacs".to_string(),
    };
    let updated = db
        .update_user_memory(user_id, MemoryRef::Key("editor".to_string()), new_content)
        .await
        .expect("failed to update memory");
    assert_eq!(updated.id, record_id::<UserMemory, _>(id));
}

#[tokio::test]
async fn should_not_update_unknown_user_memory() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let content = UserMemoryContent {
        id: record_id::<UserMemoryContent, _>(Uuid::now_v7()),
        content: "x".to_string(),
    };
    let err = db
        .update_user_memory(user_id, MemoryRef::Key("missing".to_string()), content)
        .await
        .expect_err("updated an unknown memory");
    assert!(matches!(err, StorageError::NotFound));
}

#[tokio::test]
async fn should_forget_user_memory_with_content() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let id = Uuid::now_v7();
    let (memory, content) = user_memory_for(id, user_id, Some("editor"), "vim");
    db.record_user_memory(user_id, memory, content)
        .await
        .expect("failed to record memory");

    db.forget_user_memory(user_id, id)
        .await
        .expect("failed to forget memory");

    assert!(
        db.get_user_memory(user_id, id)
            .await
            .expect("failed to get memory")
            .is_none(),
        "memory still present after forget"
    );
    assert_eq!(
        count::<UserMemoryContent>(&db).await,
        0,
        "memory content not forgotten"
    );
}

#[tokio::test]
async fn should_not_get_user_memory_of_other_user() {
    let db = memory_db().await;
    let owner = Uuid::now_v7();
    db.create_user(user(owner))
        .await
        .expect("failed to create owner");
    let id = Uuid::now_v7();
    let (memory, content) = user_memory_for(id, owner, Some("editor"), "vim");
    db.record_user_memory(owner, memory, content)
        .await
        .expect("failed to record memory");

    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    assert!(
        db.get_user_memory(other, id)
            .await
            .expect("failed to get memory")
            .is_none(),
        "memory of another user disclosed"
    );
}

#[tokio::test]
async fn should_upsert_keyed_context_memory_in_place() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (first, first_content) =
        context_memory_for(Uuid::now_v7(), session_id, user_id, Some("goal"), "a");
    db.record_context_memory(user_id, first, first_content)
        .await
        .expect("failed to record first context memory");
    let (second, second_content) =
        context_memory_for(Uuid::now_v7(), session_id, user_id, Some("goal"), "b");
    db.record_context_memory(user_id, second, second_content)
        .await
        .expect("failed to record second context memory");

    let all = db
        .list_context_memory(user_id, session_id)
        .await
        .expect("failed to list context memory");
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn should_list_user_memory_with_content_newest_first() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let (older, older_content) = user_memory_for(Uuid::now_v7(), user_id, Some("editor"), "vim");
    let mut older = older;
    older.updated_at = Utc::now() - Duration::minutes(5);
    db.record_user_memory(user_id, older, older_content)
        .await
        .expect("failed to record older memory");

    let (newer, newer_content) = user_memory_for(Uuid::now_v7(), user_id, None, "prefers tabs");
    db.record_user_memory(user_id, newer, newer_content)
        .await
        .expect("failed to record newer memory");

    let listed = db
        .list_user_memory_with_content(user_id)
        .await
        .expect("failed to list memory with content");

    assert_eq!(listed.len(), 2);
    // Most recently updated first, each paired with its own content.
    assert_eq!(listed[0].0.key, None);
    assert_eq!(listed[0].1.content, "prefers tabs");
    assert_eq!(listed[1].0.key.as_deref(), Some("editor"));
    assert_eq!(listed[1].1.content, "vim");
}

#[tokio::test]
async fn should_not_list_user_memory_with_content_of_other_user() {
    let db = memory_db().await;
    let owner = Uuid::now_v7();
    db.create_user(user(owner))
        .await
        .expect("failed to create owner");
    let (memory, content) = user_memory_for(Uuid::now_v7(), owner, Some("editor"), "vim");
    db.record_user_memory(owner, memory, content)
        .await
        .expect("failed to record memory");

    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let listed = db
        .list_user_memory_with_content(other)
        .await
        .expect("failed to list memory with content");
    assert!(listed.is_empty(), "memory of another user disclosed");
}

#[tokio::test]
async fn should_list_context_memory_with_content_scoped_to_session() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (memory, content) =
        context_memory_for(Uuid::now_v7(), session_id, user_id, Some("goal"), "ship it");
    db.record_context_memory(user_id, memory, content)
        .await
        .expect("failed to record context memory");

    // A second session of the same user must not leak into the first's list.
    let other_session = Uuid::now_v7();
    db.create_session(session_for(other_session, user_id))
        .await
        .expect("failed to create second session");
    let (other, other_content) =
        context_memory_for(Uuid::now_v7(), other_session, user_id, None, "elsewhere");
    db.record_context_memory(user_id, other, other_content)
        .await
        .expect("failed to record other context memory");

    let listed = db
        .list_context_memory_with_content(user_id, session_id)
        .await
        .expect("failed to list context memory with content");

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].0.key.as_deref(), Some("goal"));
    assert_eq!(listed[0].1.content, SecretContent::plaintext("ship it"));
}

#[tokio::test]
async fn should_list_context_memory_with_sealed_content() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let id = Uuid::now_v7();
    let (memory, _) = context_memory_for(id, session_id, user_id, Some("goal"), "");
    let sealed = ContentEnvelope {
        version: 1,
        algorithm: "xchacha20poly1305".to_string(),
        key_id: "kf_ab12".to_string(),
        nonce: "bm9uY2U".to_string(),
        ciphertext: "Y2lwaGVydGV4dA".to_string(),
    };
    let content = ContextMemoryContent {
        id: record_id::<ContextMemoryContent, _>(id),
        content: SecretContent::Encrypted(sealed.clone()),
    };
    db.record_context_memory(user_id, memory, content)
        .await
        .expect("failed to record sealed context memory");

    let listed = db
        .list_context_memory_with_content(user_id, session_id)
        .await
        .expect("failed to list context memory with content");

    // Storage returns the envelope opaquely; it never decrypts.
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].1.content, SecretContent::Encrypted(sealed));
}

#[tokio::test]
async fn should_not_record_context_memory_for_unowned_session() {
    let db = memory_db().await;
    let (_owner, session_id) = user_with_session(&db).await;
    let other = Uuid::now_v7();
    db.create_user(user(other))
        .await
        .expect("failed to create other user");

    let (memory, content) = context_memory_for(Uuid::now_v7(), session_id, other, None, "x");
    let err = db
        .record_context_memory(other, memory, content)
        .await
        .expect_err("recorded context memory for an unowned session");
    assert!(matches!(err, StorageError::NotFound));
}

// -- Retention / cleanup ----------------------------------------------------

/// Builds a session with explicit `updated_at` and `archived_at`, so retention
/// windows can be exercised deterministically.
fn session_with(
    id: Uuid,
    user_id: Uuid,
    updated_at: chrono::DateTime<Utc>,
    archived_at: Option<chrono::DateTime<Utc>>,
) -> Session {
    Session {
        id: record_id::<Session, _>(id),
        user: record_id::<User, _>(user_id),
        title: Some("session".to_string()),
        encrypted: false,
        key_id: None,
        created_at: Utc::now(),
        updated_at,
        archived_at,
    }
}

#[tokio::test]
async fn should_delete_expired_and_revoked_tokens() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    let live_id = Uuid::now_v7();
    db.create_token(token_for(
        live_id,
        user_id,
        "live",
        Utc::now() + Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create live token");
    db.create_token(token_for(
        Uuid::now_v7(),
        user_id,
        "expired",
        Utc::now() - Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create expired token");
    db.create_token(token_for(
        Uuid::now_v7(),
        user_id,
        "revoked",
        Utc::now() + Duration::hours(1),
        Some(Utc::now()),
    ))
    .await
    .expect("failed to create revoked token");

    db.delete_expired_tokens()
        .await
        .expect("failed to delete expired tokens");

    // Only the live token survives.
    assert_eq!(
        count::<AuthToken>(&db).await,
        1,
        "expired or revoked tokens remain"
    );
    assert!(
        db.get_active_token(live_id)
            .await
            .expect("failed to get token")
            .is_some(),
        "live token was deleted"
    );
}

#[tokio::test]
async fn should_purge_old_sessions_and_children() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    // An old, untouched session past the retention window, with a child row.
    let old_id = Uuid::now_v7();
    db.create_session(session_with(
        old_id,
        user_id,
        Utc::now() - Duration::days(40),
        None,
    ))
    .await
    .expect("failed to create old session");
    let (message, content) = message_for(Uuid::now_v7(), old_id, user_id);
    db.append_message(user_id, message, content)
        .await
        .expect("failed to append message");

    // A recent session inside the window.
    let recent_id = Uuid::now_v7();
    db.create_session(session_with(recent_id, user_id, Utc::now(), None))
        .await
        .expect("failed to create recent session");

    db.purge_old_sessions(30)
        .await
        .expect("failed to purge old sessions");

    assert!(
        db.get_session(user_id, old_id)
            .await
            .expect("failed to get session")
            .is_none(),
        "old session not purged"
    );
    assert!(
        db.get_session(user_id, recent_id)
            .await
            .expect("failed to get session")
            .is_some(),
        "recent session was purged"
    );
    assert_eq!(
        count::<SessionMessage>(&db).await,
        0,
        "child rows of old session remain"
    );
    assert_eq!(
        count::<SessionMessageContent>(&db).await,
        0,
        "child content of old session remains"
    );
}

#[tokio::test]
async fn should_not_purge_archived_session_as_old() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    // Old, but archived: it is the archived purge's responsibility, not this one.
    let id = Uuid::now_v7();
    db.create_session(session_with(
        id,
        user_id,
        Utc::now() - Duration::days(40),
        Some(Utc::now()),
    ))
    .await
    .expect("failed to create archived session");

    db.purge_old_sessions(30)
        .await
        .expect("failed to purge old sessions");

    assert!(
        db.get_session(user_id, id)
            .await
            .expect("failed to get session")
            .is_some(),
        "archived session was purged as old"
    );
}

#[tokio::test]
async fn should_purge_old_archived_sessions() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");

    // Archived long ago, past the window.
    let old_id = Uuid::now_v7();
    db.create_session(session_with(
        old_id,
        user_id,
        Utc::now(),
        Some(Utc::now() - Duration::days(40)),
    ))
    .await
    .expect("failed to create old archived session");

    // Archived recently, inside the window.
    let recent_id = Uuid::now_v7();
    db.create_session(session_with(
        recent_id,
        user_id,
        Utc::now(),
        Some(Utc::now()),
    ))
    .await
    .expect("failed to create recent archived session");

    db.purge_archived_sessions(30)
        .await
        .expect("failed to purge archived sessions");

    assert!(
        db.get_session(user_id, old_id)
            .await
            .expect("failed to get session")
            .is_none(),
        "old archived session not purged"
    );
    assert!(
        db.get_session(user_id, recent_id)
            .await
            .expect("failed to get session")
            .is_some(),
        "recently archived session was purged"
    );
}

#[tokio::test]
async fn should_purge_old_traces() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (old, old_content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        user_id,
        Provider::OpenAI,
        "gpt",
        Utc::now() - Duration::days(40),
    );
    db.append_trace_event(user_id, old, old_content)
        .await
        .expect("failed to append old trace event");

    let (recent, recent_content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        user_id,
        Provider::Anthropic,
        "claude",
        Utc::now(),
    );
    db.append_trace_event(user_id, recent, recent_content)
        .await
        .expect("failed to append recent trace event");

    db.purge_traces(30).await.expect("failed to purge traces");

    assert_eq!(
        count::<TraceEvent>(&db).await,
        1,
        "old trace event not purged"
    );
    assert_eq!(
        count::<TraceEventContent>(&db).await,
        1,
        "old trace content not purged"
    );
    // The session itself outlives a trace purge.
    assert!(
        db.get_session(user_id, session_id)
            .await
            .expect("failed to get session")
            .is_some(),
        "session was purged by trace cleanup"
    );
}

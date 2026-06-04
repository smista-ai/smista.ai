use chrono::{Duration, Utc};
use smista_core::intent::TaskIntent;
use smista_core::message::MessageRole;
use smista_core::model::Provider;
use surrealdb::types::RecordId;

use super::*;
use crate::entity::{ToolCallStatus, TraceEventType};

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
async fn should_get_user_by_api_key_hash() {
    let db = memory_db().await;

    let id = Uuid::now_v7();
    let user = user(id);

    assert!(
        db.get_user_by_api_key_hash(&user.api_key_hash)
            .await
            .expect("failed to get user by API key hash")
            .is_none(),
        "unexpectedly found user before creation"
    );

    db.create_user(user.clone())
        .await
        .expect("failed to create user");

    let got = db
        .get_user_by_api_key_hash(&user.api_key_hash)
        .await
        .expect("failed to get user by API key hash")
        .expect("user not found after creation");

    assert_eq!(got, user);
}

#[tokio::test]
async fn should_not_get_nonexistent_user_by_api_key_hash() {
    let db = memory_db().await;

    assert!(
        db.get_user_by_api_key_hash("hash")
            .await
            .expect("failed to get user by API key hash")
            .is_none(),
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
            content: "hello".to_string(),
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
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
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

#[allow(clippy::too_many_arguments)]
fn trace_event_for(
    id: Uuid,
    session_id: Uuid,
    user_id: Uuid,
    provider: Provider,
    model: &str,
    created_at: chrono::DateTime<Utc>,
    payload: &str,
) -> (TraceEvent, TraceEventContent) {
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
            payload: payload.to_string(),
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
            content: content.to_string(),
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
async fn should_validate_active_token() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");
    db.create_token(token_for(
        Uuid::now_v7(),
        user_id,
        "live",
        Utc::now() + Duration::hours(1),
        None,
    ))
    .await
    .expect("failed to create token");

    let valid = db
        .validate_token("live")
        .await
        .expect("failed to validate token");

    assert!(valid.is_some());
}

#[tokio::test]
async fn should_not_validate_expired_or_revoked_token() {
    let db = memory_db().await;
    let user_id = Uuid::now_v7();
    db.create_user(user(user_id))
        .await
        .expect("failed to create user");
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

    assert!(
        db.validate_token("expired")
            .await
            .expect("failed to validate")
            .is_none(),
        "expired token validated"
    );
    assert!(
        db.validate_token("revoked")
            .await
            .expect("failed to validate")
            .is_none(),
        "revoked token validated"
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
        db.validate_token("to-revoke")
            .await
            .expect("failed to validate")
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
        "{}",
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
async fn should_assemble_latest_trace_from_events() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    let (first, first_content) = trace_event_for(
        Uuid::now_v7(),
        session_id,
        user_id,
        Provider::OpenAI,
        "gpt",
        Utc::now() - Duration::minutes(5),
        "{\"step\":1}",
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
        "{\"step\":2}",
    );
    db.append_trace_event(user_id, second, second_content)
        .await
        .expect("failed to append second event");

    let trace = db
        .get_latest_trace(user_id, session_id)
        .await
        .expect("failed to load trace")
        .expect("trace not found");

    assert_eq!(trace.session_id, session_id);
    assert_eq!(trace.events.len(), 2);
    // Events are ordered oldest first; each carries its own routing context.
    assert_eq!(trace.events[0].provider, Provider::OpenAI);
    assert_eq!(trace.events[0].model, "gpt");
    assert_eq!(trace.events[0].payload["step"], 1);
    assert_eq!(trace.events[1].provider, Provider::Anthropic);
    assert_eq!(trace.events[1].model, "claude");
    assert_eq!(trace.events[1].payload["step"], 2);
}

#[tokio::test]
async fn should_return_no_trace_without_events() {
    let db = memory_db().await;
    let (user_id, session_id) = user_with_session(&db).await;

    assert!(
        db.get_latest_trace(user_id, session_id)
            .await
            .expect("failed to load trace")
            .is_none(),
        "trace assembled from no events"
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

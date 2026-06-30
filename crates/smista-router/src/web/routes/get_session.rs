//! `GET /api/v1/sessions/{session_id}` — fetch or resume a session.
//!
//! Returns the full session, including its messages and metadata, as a
//! [`GetSessionResponse`](smista_core::api::GetSessionResponse).
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and the lookup is scoped to that user. A session
//! owned by another user is treated as absent and never disclosed, so it yields
//! the same `404` as an unknown id; an archived session is treated as gone and
//! also yields `404`.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{
    ApiErrorCode, GetSessionResponse, MessageContent, SessionDetail, SessionMessageDetail,
};
use smista_core::message::MessageRole;
use smista_storage::types::SecretContent;
use uuid::Uuid;

use crate::session::{SessionError, Sessions};
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `GET /api/v1/sessions/{session_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}",
        operation_id = "getSession",
        tag = "sessions",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id")
        ),
        responses(
            (status = 200, description = "Session detail", body = smista_core::api::GetSessionResponse),
            (status = 400, description = "Invalid session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found or no access to session", body = smista_core::api::ApiError),
            (status = 500, description = "Internal server error", body = smista_core::api::ApiError),
        )
    )
)]
pub(crate) async fn get_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
) -> ApiResult<GetSessionResponse> {
    let sessions = Sessions::new(state.database.clone(), user.user_id);
    let Ok(session_id) = Uuid::parse_str(&session_id) else {
        return Err(WebError::from_code(
            ApiErrorCode::InvalidSessionId,
            "Invalid session id.",
        ));
    };

    // `open` verifies the session exists and is owned by this user, treating a
    // non-owned session as absent so its existence is never disclosed.
    let user_session = match sessions.open(session_id).await {
        Ok(session) => session,
        Err(SessionError::NotFound) => {
            tracing::debug!(
                "Session not found or no access to session: {:?}",
                session_id
            );
            return Err(WebError::from_code(
                ApiErrorCode::SessionNotFound,
                "Session not found or no access to session.",
            ));
        }
        Err(SessionError::Database(err)) => {
            tracing::error!("Database error while opening session: {:?}", err);
            return Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "An internal error occurred.",
            ));
        }
    };

    let session = match user_session.session().await {
        Ok(Some(session)) => session,
        // The session was opened a moment ago; its disappearance is a race, not
        // a normal outcome, so report it as not found.
        Ok(None) => {
            tracing::debug!("Session vanished after it was opened: {:?}", session_id);
            return Err(WebError::from_code(
                ApiErrorCode::SessionNotFound,
                "Session not found or no access to session.",
            ));
        }
        Err(err) => {
            tracing::error!("Database error while fetching session: {:?}", err);
            return Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "An internal error occurred.",
            ));
        }
    };

    // Storage still returns archived sessions; this endpoint treats them as gone
    // and filters them out, so an archived session is indistinguishable from one
    // that never existed.
    if session.archived_at.is_some() {
        tracing::debug!("Refusing to return archived session: {:?}", session_id);
        return Err(WebError::from_code(
            ApiErrorCode::SessionNotFound,
            "Session not found or no access to session.",
        ));
    }

    let messages = match user_session.messages().await {
        Ok(messages) => messages,
        Err(err) => {
            tracing::error!("Database error while fetching session messages: {:?}", err);
            return Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "An internal error occurred.",
            ));
        }
    };

    let messages = messages
        .into_iter()
        .map(|(message, content)| {
            // Provider and model identify an assistant turn's origin and carry no
            // meaning for the other roles, so they are reported only there.
            let (provider, model) = match message.role {
                MessageRole::Assistant => (Some(message.provider), Some(message.model)),
                MessageRole::System | MessageRole::User | MessageRole::Tool => (None, None),
            };
            SessionMessageDetail {
                role: message.role,
                content: into_message_content(content.content),
                provider,
                model,
            }
        })
        .collect();

    let detail = SessionDetail {
        id: session_id,
        title: session.title.unwrap_or_default(),
        scope: session.scope,
        encrypted: session.encrypted,
        created_at: session.created_at,
        updated_at: session.updated_at,
        messages,
        // No per-session metadata is persisted yet; the field is always present
        // and empty so clients can rely on its shape.
        metadata: serde_json::json!({}),
    };

    Ok((StatusCode::OK, Json(GetSessionResponse { session: detail })))
}

/// Converts a stored [`SecretContent`] payload into its wire [`MessageContent`].
///
/// A plaintext payload is carried through as clear text; an encrypted payload is
/// returned as its sealed envelope, which only a client holding the session key
/// can open.
fn into_message_content(content: SecretContent) -> MessageContent {
    match content {
        SecretContent::Plaintext(text) => MessageContent::Plaintext(text),
        SecretContent::Encrypted(envelope) => MessageContent::Encrypted(envelope.into()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::{Duration, Utc};
    use smista_core::message::MessageRole;
    use smista_core::model::Provider;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::SurrealDatabase;
    use smista_storage::entity::{
        Session, SessionMessage, SessionMessageContent, Table as _, User,
    };
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::{ContentEnvelope, SecretContent};
    use uuid::Uuid;

    use crate::session::Sessions;
    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_database, get, get_with_token, send,
    };

    /// Creates a session owned by `user_id` and returns its id.
    async fn create_session(
        db: &SurrealDatabase,
        user_id: Uuid,
        title: &str,
        key_id: Option<String>,
    ) -> Uuid {
        let sessions = Sessions::new(db.clone(), user_id);
        let session_id = Uuid::now_v7();
        sessions
            .create(Session::new(
                session_id,
                user_id,
                Some(title.to_string()),
                None,
                key_id,
            ))
            .await
            .expect("failed to create session");
        session_id
    }

    /// Appends one message to `session_id`, stamped at `created_at` so a test can
    /// pin message ordering.
    async fn append_message(
        db: &SurrealDatabase,
        user_id: Uuid,
        session_id: Uuid,
        role: MessageRole,
        content: SecretContent,
        created_at: chrono::DateTime<Utc>,
    ) {
        let session = Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session");
        let id = Uuid::now_v7();
        session
            .append_message(
                SessionMessage {
                    id: RecordId::new(SessionMessage::name(), id.to_string()),
                    session: RecordId::new(Session::name(), session_id.to_string()),
                    user: RecordId::new(User::name(), user_id.to_string()),
                    role,
                    provider: Provider::Anthropic,
                    model: "claude-sonnet".to_string(),
                    created_at,
                },
                SessionMessageContent {
                    id: RecordId::new(SessionMessageContent::name(), id.to_string()),
                    content,
                },
            )
            .await
            .expect("failed to append message");
    }

    fn envelope() -> ContentEnvelope {
        ContentEnvelope {
            version: 1,
            algorithm: "xchacha20poly1305".to_string(),
            key_id: "kf_ab12".to_string(),
            nonce: "bm9uY2U".to_string(),
            ciphertext: "Y2lwaGVydGV4dA".to_string(),
        }
    }

    #[tokio::test]
    async fn should_return_the_session_with_its_messages_in_order() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Refactor auth middleware", None).await;

        let earlier = Utc::now();
        append_message(
            &db,
            user_id,
            session_id,
            MessageRole::User,
            SecretContent::plaintext("Refactor the auth middleware."),
            earlier,
        )
        .await;
        append_message(
            &db,
            user_id,
            session_id,
            MessageRole::Assistant,
            SecretContent::plaintext("Here is the plan..."),
            earlier + Duration::seconds(1),
        )
        .await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let session = &body["session"];
        assert_eq!(session["id"], session_id.to_string());
        assert_eq!(session["title"], "Refactor auth middleware");
        assert_eq!(session["encrypted"], false);
        // Metadata is always present, even when empty.
        assert_eq!(session["metadata"], serde_json::json!({}));

        let messages = session["messages"].as_array().expect("messages missing");
        assert_eq!(messages.len(), 2);
        // Oldest first: the user turn precedes the assistant turn.
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(
            messages[0]["content"]["plaintext"],
            "Refactor the auth middleware."
        );
        // A user turn carries no provider or model.
        assert!(messages[0].get("provider").is_none());
        assert!(messages[0].get("model").is_none());
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"]["plaintext"], "Here is the plan...");
        // An assistant turn reports the model that produced it.
        assert_eq!(messages[1]["provider"], "anthropic");
        assert_eq!(messages[1]["model"], "claude-sonnet");
    }

    #[tokio::test]
    async fn should_return_an_empty_message_list_and_metadata_for_a_fresh_session() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Fresh", None).await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["messages"], serde_json::json!([]));
        assert_eq!(body["session"]["metadata"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn should_return_sealed_content_for_an_encrypted_session() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "secret", Some("kf_ab12".to_string())).await;
        append_message(
            &db,
            user_id,
            session_id,
            MessageRole::User,
            SecretContent::from(envelope()),
            Utc::now(),
        )
        .await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["encrypted"], true);
        let message = &body["session"]["messages"][0];
        // The router holds no key, so it returns the sealed envelope verbatim.
        assert!(message["content"].get("plaintext").is_none());
        assert_eq!(message["content"]["encrypted"]["key_id"], "kf_ab12");
        assert_eq!(
            message["content"]["encrypted"]["ciphertext"],
            "Y2lwaGVydGV4dA"
        );
    }

    #[tokio::test]
    async fn should_treat_a_session_owned_by_another_user_as_not_found() {
        let (router, token, _user_id, db) = authenticated_router_with_database().await;

        // A second user owns a session the authenticated caller must not see.
        let other_id = Uuid::now_v7();
        db.create_user(User {
            id: RecordId::new(User::name(), other_id.to_string()),
            api_key_hash: "hash".to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("failed to create other user");
        let session_id = create_session(&db, other_id, "not yours", None).await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        // Reported as not found, never as forbidden, so existence stays private.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_treat_an_archived_session_as_not_found() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "archived", None).await;
        Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session")
            .archive()
            .await
            .expect("failed to archive session");

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_return_not_found_for_an_unknown_session() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{}", Uuid::now_v7()), &token),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_reject_a_malformed_session_id() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            get_with_token("/api/v1/sessions/not-a-uuid", &token),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_session_id");
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) =
            send(router, get(&format!("/api/v1/sessions/{}", Uuid::now_v7()))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}

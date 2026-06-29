//! `DELETE /api/v1/sessions/{session_id}` — delete a session.
//!
//! Deletes the session and its context memory, returning a
//! [`DeleteSessionResponse`](smista_core::api::DeleteSessionResponse).
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and the delete is scoped to that user. A session
//! owned by another user is treated as absent and never disclosed, so it yields
//! the same `404` as an unknown id. The delete cascades to every child row,
//! including the session's context memory, so nothing is left orphaned.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, DeleteSessionResponse};
use uuid::Uuid;

use crate::session::{SessionError, Sessions};
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `DELETE /api/v1/sessions/{session_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        delete,
        path = "/api/v1/sessions/{session_id}",
        operation_id = "deleteSession",
        tag = "sessions",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id")
        ),
        responses(
            (status = 200, description = "Session deleted", body = smista_core::api::DeleteSessionResponse),
            (status = 400, description = "Invalid session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found or no access to session", body = smista_core::api::ApiError),
            (status = 500, description = "Internal server error", body = smista_core::api::ApiError),
        )
    )
)]
pub(crate) async fn delete_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
) -> ApiResult<DeleteSessionResponse> {
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

    user_session.delete().await.map_err(|err| {
        tracing::error!("Database error while deleting session: {:?}", err);
        WebError::from_code(ApiErrorCode::InternalError, "An internal error occurred.")
    })?;

    Ok((
        StatusCode::OK,
        Json(DeleteSessionResponse { deleted: true }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::Utc;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::SurrealDatabase;
    use smista_storage::entity::{ContextMemory, ContextMemoryContent, Session, Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::SecretContent;
    use uuid::Uuid;

    use crate::session::Sessions;
    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_database, delete, delete_with_token, send,
    };

    /// Creates a session owned by `user_id` and returns its id.
    async fn create_session(db: &SurrealDatabase, user_id: Uuid, title: &str) -> Uuid {
        let session_id = Uuid::now_v7();
        Sessions::new(db.clone(), user_id)
            .create(Session::new(
                session_id,
                user_id,
                Some(title.to_string()),
                None,
            ))
            .await
            .expect("failed to create session");
        session_id
    }

    /// Records one context memory against `session_id` so a test can assert the
    /// delete cascades to it.
    async fn record_context_memory(db: &SurrealDatabase, user_id: Uuid, session_id: Uuid) {
        let memory_id = Uuid::now_v7();
        db.record_context_memory(
            user_id,
            ContextMemory::new(memory_id, session_id, user_id, Some("topic".to_string())),
            ContextMemoryContent::new(memory_id, SecretContent::plaintext("fact")),
        )
        .await
        .expect("failed to record context memory");
    }

    #[tokio::test]
    async fn should_delete_a_session_and_report_it_deleted() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Refactor auth middleware").await;

        let (status, body) = send(
            router,
            delete_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["deleted"], true);

        // The session is gone for good: a later lookup finds nothing.
        assert!(
            Sessions::new(db.clone(), user_id)
                .get(session_id)
                .await
                .expect("failed to get session")
                .is_none(),
            "session survived the delete"
        );
    }

    #[tokio::test]
    async fn should_remove_the_associated_context_memory() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "with memory").await;
        record_context_memory(&db, user_id, session_id).await;

        // The memory exists before the delete.
        assert_eq!(
            db.list_context_memory_with_content(user_id, session_id)
                .await
                .expect("failed to list context memory")
                .len(),
            1
        );

        let (status, _body) = send(
            router,
            delete_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // The cascade leaves no orphaned context memory behind.
        assert!(
            db.list_context_memory_with_content(user_id, session_id)
                .await
                .expect("failed to list context memory")
                .is_empty(),
            "context memory survived the delete"
        );
    }

    #[tokio::test]
    async fn should_treat_a_session_owned_by_another_user_as_not_found() {
        let (router, token, _user_id, db) = authenticated_router_with_database().await;

        // A second user owns a session the authenticated caller must not delete.
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
        let session_id = create_session(&db, other_id, "not yours").await;

        let (status, body) = send(
            router,
            delete_with_token(&format!("/api/v1/sessions/{session_id}"), &token),
        )
        .await;

        // Reported as not found, never as forbidden, so existence stays private.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");

        // The owner's session is untouched by the rejected delete.
        assert!(
            Sessions::new(db.clone(), other_id)
                .get(session_id)
                .await
                .expect("failed to get session")
                .is_some(),
            "a non-owner's delete removed the session"
        );
    }

    #[tokio::test]
    async fn should_return_not_found_for_an_unknown_session() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            delete_with_token(&format!("/api/v1/sessions/{}", Uuid::now_v7()), &token),
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
            delete_with_token("/api/v1/sessions/not-a-uuid", &token),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_session_id");
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(
            router,
            delete(&format!("/api/v1/sessions/{}", Uuid::now_v7())),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}

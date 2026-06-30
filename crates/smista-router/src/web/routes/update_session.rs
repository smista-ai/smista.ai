//! `PUT /api/v1/sessions/{session_id}` — update a session's title or archive it.
//!
//! Takes a partial
//! [`UpdateSessionRequest`](smista_core::api::UpdateSessionRequest) and returns
//! the updated session summary in an
//! [`UpdateSessionResponse`](smista_core::api::UpdateSessionResponse). The body
//! is partial: a field that is omitted is left unchanged, so a client can rename
//! a session without touching its archive state, and vice versa. Every
//! successful update refreshes `updated_at`.
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and the update is scoped to that user. A session
//! owned by another user is treated as absent and never disclosed, so it yields
//! the same `404` as an unknown id.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, SessionSummary, UpdateSessionRequest, UpdateSessionResponse};
use uuid::Uuid;

use crate::session::{SessionError, Sessions};
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `PUT /api/v1/sessions/{session_id}`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        put,
        path = "/api/v1/sessions/{session_id}",
        operation_id = "updateSession",
        tag = "sessions",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id")
        ),
        request_body = smista_core::api::UpdateSessionRequest,
        responses(
            (status = 200, description = "Updated session summary", body = smista_core::api::UpdateSessionResponse),
            (status = 400, description = "Invalid session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found or no access to session", body = smista_core::api::ApiError),
            (status = 500, description = "Internal server error", body = smista_core::api::ApiError),
        )
    )
)]
pub(crate) async fn update_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionRequest>,
) -> ApiResult<UpdateSessionResponse> {
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

    let mut session_entity = match user_session.session().await {
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

    // The body is partial: each field changes only when it is present, so an
    // omitted `title` keeps the current title and an omitted `archived` keeps the
    // current archive state. A present `archived` of `false` restores the session
    // by clearing the timestamp.
    let now = chrono::Utc::now();
    if let Some(title) = request.title {
        session_entity.title = Some(title);
    }
    if let Some(archived) = request.archived {
        session_entity.archived_at = archived.then_some(now);
    }
    // Every successful update is a fresh write, so stamp `updated_at` regardless
    // of which fields changed.
    session_entity.updated_at = now;

    let updated = user_session.update(session_entity).await.map_err(|err| {
        tracing::error!("Database error while updating session: {:?}", err);
        WebError::from_code(ApiErrorCode::InternalError, "An internal error occurred.")
    })?;

    let summary = SessionSummary {
        id: session_id,
        title: updated.title,
        scope: updated.scope,
        encrypted: updated.encrypted,
        key_id: updated.key_id,
        created_at: updated.created_at,
        updated_at: updated.updated_at,
        archived: updated.archived_at.is_some(),
    };

    Ok((
        StatusCode::OK,
        Json(UpdateSessionResponse { session: summary }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::Utc;
    use serde_json::json;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::SurrealDatabase;
    use smista_storage::entity::{Session, Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use uuid::Uuid;

    use crate::session::Sessions;
    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_database, put, put_json_with_token, send,
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
                None,
            ))
            .await
            .expect("failed to create session");
        session_id
    }

    /// Archives the session so a test can assert restore and partial-update
    /// behaviour from an already-archived starting point.
    async fn archive_session(db: &SurrealDatabase, user_id: Uuid, session_id: Uuid) {
        Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session")
            .archive()
            .await
            .expect("failed to archive session");
    }

    /// Reads the stored session, failing the test if it is absent.
    async fn stored(db: &SurrealDatabase, user_id: Uuid, session_id: Uuid) -> Session {
        db.get_session(user_id, session_id)
            .await
            .expect("failed to read session")
            .expect("session not found")
    }

    #[tokio::test]
    async fn should_update_the_title_and_return_the_summary() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Old title").await;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "title": "New title" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let session = &body["session"];
        assert_eq!(session["id"], session_id.to_string());
        assert_eq!(session["title"], "New title");
        assert_eq!(session["archived"], false);
        // The change is durable, not just reflected in the response.
        assert_eq!(
            stored(&db, user_id, session_id).await.title,
            Some("New title".to_string())
        );
    }

    #[tokio::test]
    async fn should_leave_the_archived_state_untouched_when_only_the_title_changes() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Archived").await;
        archive_session(&db, user_id, session_id).await;

        // The body omits `archived`, so the session must stay archived even though
        // its title changes.
        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "title": "Renamed but still archived" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["title"], "Renamed but still archived");
        assert_eq!(body["session"]["archived"], true);
        assert!(
            stored(&db, user_id, session_id).await.archived_at.is_some(),
            "omitting `archived` cleared the archive state"
        );
    }

    #[tokio::test]
    async fn should_leave_the_title_untouched_when_only_the_archived_flag_changes() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Keep this title").await;

        // The body omits `title`, so the title must survive an archive.
        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "archived": true }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["title"], "Keep this title");
        assert_eq!(body["session"]["archived"], true);
        assert_eq!(
            stored(&db, user_id, session_id).await.title,
            Some("Keep this title".to_string())
        );
    }

    #[tokio::test]
    async fn should_archive_a_session_when_archived_is_true() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Active").await;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "archived": true }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["archived"], true);
        assert!(stored(&db, user_id, session_id).await.archived_at.is_some());
    }

    #[tokio::test]
    async fn should_restore_a_session_when_archived_is_false() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Archived").await;
        archive_session(&db, user_id, session_id).await;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "archived": false }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["archived"], false);
        assert!(
            stored(&db, user_id, session_id).await.archived_at.is_none(),
            "restoring left the archive timestamp behind"
        );
    }

    #[tokio::test]
    async fn should_update_both_the_title_and_the_archive_state() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Old").await;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "title": "New", "archived": true }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["session"]["title"], "New");
        assert_eq!(body["session"]["archived"], true);
        let stored = stored(&db, user_id, session_id).await;
        assert_eq!(stored.title, Some("New".to_string()));
        assert!(stored.archived_at.is_some());
    }

    #[tokio::test]
    async fn should_refresh_updated_at_on_a_successful_update() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Stamped").await;
        let before = stored(&db, user_id, session_id).await.updated_at;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "title": "Touched" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        // The returned and the persisted `updated_at` both move forward.
        let returned: chrono::DateTime<Utc> = body["session"]["updated_at"]
            .as_str()
            .expect("updated_at missing")
            .parse()
            .expect("updated_at is not an RFC 3339 timestamp");
        assert!(returned > before, "updated_at was not refreshed");
        assert!(stored(&db, user_id, session_id).await.updated_at > before);
    }

    #[tokio::test]
    async fn should_treat_a_session_owned_by_another_user_as_not_found() {
        let (router, token, _user_id, db) = authenticated_router_with_database().await;

        // A second user owns a session the authenticated caller must not update.
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
        let session_id = create_session(&db, other_id, "Not yours").await;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{session_id}"),
                &token,
                &json!({ "title": "Hijacked" }),
            ),
        )
        .await;

        // Reported as not found, never as forbidden, so existence stays private.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
        // The owner's session is untouched by the rejected update.
        assert_eq!(
            stored(&db, other_id, session_id).await.title,
            Some("Not yours".to_string()),
            "a non-owner's update changed the session"
        );
    }

    #[tokio::test]
    async fn should_return_not_found_for_an_unknown_session() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            put_json_with_token(
                &format!("/api/v1/sessions/{}", Uuid::now_v7()),
                &token,
                &json!({ "title": "Nope" }),
            ),
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
            put_json_with_token(
                "/api/v1/sessions/not-a-uuid",
                &token,
                &json!({ "title": "x" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_session_id");
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) =
            send(router, put(&format!("/api/v1/sessions/{}", Uuid::now_v7()))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}

//! `GET /api/v1/sessions` — list the caller's sessions, archived ones included.
//!
//! Returns a [`ListSessionsResponse`](smista_core::api::ListSessionsResponse)
//! wrapping each session as a
//! [`SessionSummary`](smista_core::api::SessionSummary), newest first.
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and only that user's sessions are listed.

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, ListSessionsResponse, SessionSummary};

use crate::session::Sessions;
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `GET /api/v1/sessions`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions",
        operation_id = "listSessions",
        tag = "sessions",
        security(("bearer" = [])),
        responses(
            (status = 200, description = "List of sessions", body = smista_core::api::ListSessionsResponse),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn list_sessions(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
) -> ApiResult<ListSessionsResponse> {
    let sessions = Sessions::new(state.database.clone(), user.user_id);

    let list = match sessions.list().await {
        Ok(sessions) => sessions
            .into_iter()
            .map(|session| SessionSummary {
                id: session.uuid(),
                title: session.title,
                encrypted: session.encrypted,
                key_id: session.key_id,
                created_at: session.created_at,
                updated_at: session.updated_at,
                archived: session.archived_at.is_some(),
            })
            .collect(),
        Err(err) => {
            tracing::error!(
                "Failed to list sessions for user {}: {:?}",
                user.user_id,
                err
            );
            return Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "An internal error occurred.",
            ));
        }
    };

    Ok((
        StatusCode::OK,
        Json(ListSessionsResponse { sessions: list }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::{DateTime, Duration, Utc};
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::SurrealDatabase;
    use smista_storage::entity::{Session, Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use uuid::Uuid;

    use crate::session::Sessions;
    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_database, get, get_with_token, send,
    };

    /// Creates a session owned by `user_id`, stamped at `updated_at` so a test
    /// can pin list ordering, and returns its id.
    async fn create_session_at(
        db: &SurrealDatabase,
        user_id: Uuid,
        title: &str,
        key_id: Option<String>,
        updated_at: DateTime<Utc>,
    ) -> Uuid {
        let session_id = Uuid::now_v7();
        let mut session = Session::new(session_id, user_id, Some(title.to_string()), key_id);
        session.updated_at = updated_at;
        Sessions::new(db.clone(), user_id)
            .create(session)
            .await
            .expect("failed to create session");
        session_id
    }

    #[tokio::test]
    async fn should_list_the_users_sessions_newest_first() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let now = Utc::now();
        // The older session is created first but must be listed last.
        let older = create_session_at(&db, user_id, "older", None, now - Duration::hours(1)).await;
        let newer = create_session_at(&db, user_id, "newer", None, now).await;

        let (status, body) = send(router, get_with_token("/api/v1/sessions", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let sessions = body["sessions"].as_array().expect("sessions missing");
        assert_eq!(sessions.len(), 2);
        // Ordered by most recently updated first.
        assert_eq!(sessions[0]["id"], newer.to_string());
        assert_eq!(sessions[0]["title"], "newer");
        assert_eq!(sessions[1]["id"], older.to_string());
        assert_eq!(sessions[1]["title"], "older");
    }

    #[tokio::test]
    async fn should_include_archived_sessions() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session_at(&db, user_id, "archived", None, Utc::now()).await;
        Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session")
            .archive()
            .await
            .expect("failed to archive session");

        let (status, body) = send(router, get_with_token("/api/v1/sessions", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let sessions = body["sessions"].as_array().expect("sessions missing");
        // An archived session is still listed, flagged as archived.
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["id"], session_id.to_string());
        assert_eq!(sessions[0]["archived"], true);
    }

    #[tokio::test]
    async fn should_return_an_empty_list_for_a_user_with_no_sessions() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(router, get_with_token("/api/v1/sessions", &token)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["sessions"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn should_report_the_key_id_only_for_an_encrypted_session() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let now = Utc::now();
        create_session_at(&db, user_id, "secret", Some("kf_ab12".to_string()), now).await;
        create_session_at(&db, user_id, "plain", None, now - Duration::hours(1)).await;

        let (status, body) = send(router, get_with_token("/api/v1/sessions", &token)).await;

        assert_eq!(status, StatusCode::OK);
        // Newest first: the encrypted session leads and carries its key id.
        let encrypted = &body["sessions"][0];
        assert_eq!(encrypted["encrypted"], true);
        assert_eq!(encrypted["key_id"], "kf_ab12");
        // A plaintext session omits the key id entirely.
        let plaintext = &body["sessions"][1];
        assert_eq!(plaintext["encrypted"], false);
        assert!(plaintext.get("key_id").is_none());
    }

    #[tokio::test]
    async fn should_not_list_another_users_sessions() {
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
        create_session_at(&db, other_id, "not yours", None, Utc::now()).await;

        let (status, body) = send(router, get_with_token("/api/v1/sessions", &token)).await;

        assert_eq!(status, StatusCode::OK);
        // The caller has no sessions; the other user's session never leaks.
        assert_eq!(body["sessions"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(router, get("/api/v1/sessions")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}

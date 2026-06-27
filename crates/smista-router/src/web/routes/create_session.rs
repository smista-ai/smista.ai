//! `POST /api/v1/sessions` — create a session.
//!
//! Takes a [`CreateSessionRequest`](smista_core::api::CreateSessionRequest) and
//! returns `201` with a
//! [`CreateSessionResponse`](smista_core::api::CreateSessionResponse).
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and the new session is scoped to that user. A
//! session is end-to-end encrypted when, and only when, the request carries a
//! `key_id`; there is no separate `encrypted` flag.

use axum::extract::State;
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, CreateSessionRequest, CreateSessionResponse, SessionSummary};
use smista_storage::entity::Session;
use uuid::Uuid;

use crate::session::Sessions;
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `POST /api/v1/sessions`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions",
        operation_id = "createSession",
        tag = "sessions",
        security(("bearer" = [])),
        request_body = smista_core::api::CreateSessionRequest,
        responses(
            (status = 201, description = "Session created", body = smista_core::api::CreateSessionResponse),
            (status = 400, description = "Invalid request", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 500, description = "Internal server error", body = smista_core::api::ApiError),
        )
    )
)]
pub(crate) async fn create_session(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<CreateSessionRequest>,
) -> ApiResult<CreateSessionResponse> {
    let sessions = Sessions::new(state.database.clone(), user.user_id);

    let session = Session::new(
        Uuid::now_v7(),
        user.user_id,
        Some(request.title),
        request.key_id,
    );

    let user_session = match sessions.create(session).await {
        Ok(user_session) => user_session,
        Err(err) => {
            tracing::error!("Database error while creating session: {:?}", err);
            return Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "An internal error occurred.",
            ));
        }
    };

    let session = match user_session.session().await {
        Ok(Some(session_data)) => session_data,
        Ok(None) => {
            tracing::error!(
                "Session not found after creation: {:?}",
                user_session.session_id()
            );
            return Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "An internal error occurred.",
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

    let summary = SessionSummary {
        id: user_session.session_id(),
        title: session.title,
        encrypted: session.encrypted,
        key_id: session.key_id,
        created_at: session.created_at,
        updated_at: session.updated_at,
        archived: session.archived_at.is_some(),
    };

    Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse { session: summary }),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::json;
    use smista_storage::database::Database as _;
    use smista_storage::entity::{Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use uuid::Uuid;

    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_database, post, post_json_with_token, send,
        send_status,
    };

    #[tokio::test]
    async fn should_create_a_session_and_return_201_with_the_summary() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions",
                &token,
                &json!({ "title": "Refactor auth middleware" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);

        let session = &body["session"];
        assert_eq!(session["title"], "Refactor auth middleware");
        // No `key_id` was sent, so the session is not end-to-end encrypted.
        assert_eq!(session["encrypted"], false);
        assert_eq!(session["archived"], false);
        // A new session is created and updated at the same instant.
        assert_eq!(session["created_at"], session["updated_at"]);
        // The id is a valid UUID the caller can address the session by.
        let id = session["id"].as_str().expect("id missing");
        Uuid::parse_str(id).expect("id is not a valid UUID");
    }

    #[tokio::test]
    async fn should_persist_the_session_owned_by_the_authenticated_user() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            post_json_with_token("/api/v1/sessions", &token, &json!({ "title": "Persisted" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        let session_id =
            Uuid::parse_str(body["session"]["id"].as_str().expect("id missing")).expect("bad uuid");
        let stored = db
            .get_session(user_id, session_id)
            .await
            .expect("failed to read session")
            .expect("created session was not persisted");

        assert_eq!(stored.title, Some("Persisted".to_string()));
        assert_eq!(
            stored.user,
            RecordId::new(User::name(), user_id.to_string())
        );
        assert!(!stored.encrypted);
        assert!(stored.archived_at.is_none());
    }

    #[tokio::test]
    async fn should_create_an_encrypted_session_when_a_key_id_is_supplied() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions",
                &token,
                &json!({ "title": "secret", "key_id": "kf_ab12" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        // A supplied key id alone marks the session encrypted: the request
        // carries no separate `encrypted` flag.
        assert_eq!(body["session"]["encrypted"], true);

        let session_id =
            Uuid::parse_str(body["session"]["id"].as_str().expect("id missing")).expect("bad uuid");
        let stored = db
            .get_session(user_id, session_id)
            .await
            .expect("failed to read session")
            .expect("created session was not persisted");
        assert!(stored.encrypted);
        assert_eq!(stored.key_id, Some("kf_ab12".to_string()));
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(router, post("/api/v1/sessions")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_title() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        // A body that omits the required `title` field fails to deserialize into
        // the request type, which axum reports as an unprocessable entity with a
        // plain-text body.
        let status = send_status(
            router,
            post_json_with_token("/api/v1/sessions", &token, &json!({})),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn should_not_disclose_a_session_to_another_user() {
        // A session created by one user must not be readable by another over the
        // same database, so creation scopes ownership to the authenticated user.
        let (router, token, user_id, db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            post_json_with_token("/api/v1/sessions", &token, &json!({ "title": "private" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let session_id =
            Uuid::parse_str(body["session"]["id"].as_str().expect("id missing")).expect("bad uuid");

        let intruder_id = Uuid::now_v7();
        db.create_user(User {
            id: RecordId::new(User::name(), intruder_id.to_string()),
            api_key_hash: "hash".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("failed to create intruder");

        assert!(
            db.get_session(intruder_id, session_id)
                .await
                .expect("failed to read session")
                .is_none(),
            "session disclosed to a non-owner"
        );
        // The stored row still names the original owner.
        let stored = db
            .get_session(user_id, session_id)
            .await
            .expect("failed to read session")
            .expect("session not found for its owner");
        assert_eq!(stored.uuid(), session_id);
    }
}

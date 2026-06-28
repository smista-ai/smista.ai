//! `GET /api/v1/sessions/{session_id}/traces` — fetch a session's trace.
//!
//! Returns the session's ordered trace events, windowed by the `limit` and
//! `offset` query parameters, as a
//! [`TraceResponse`](smista_core::api::TraceResponse).
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and the lookup is scoped to that user. An
//! archived session, or one owned by another user, is treated as absent and
//! never disclosed, so it yields the same `404` as an unknown id.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, TraceResponse};
use smista_storage::api::Pagination;
use uuid::Uuid;

use crate::trace::Tracer;
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub(crate) struct TraceQueryParams {
    /// Maximum number of events to return.
    limit: Option<u64>,
    /// Number of leading events to skip.
    offset: Option<u64>,
}

/// Handles `GET /api/v1/sessions/{session_id}/traces`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}/traces",
        operation_id = "getSessionTraces",
        tag = "traces",
        security(("bearer" = [])),
        params(
            ("session_id" = String, Path, description = "Session id"),
            ("limit" = Option<u64>, Query, description = "Maximum number of events to return"),
            ("offset" = Option<u64>, Query, description = "Number of leading events to skip")
        ),
        responses(
            (status = 200, description = "Routing trace for the session", body = smista_core::api::TraceResponse),
            (status = 400, description = "Invalid session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError),
            (status = 500, description = "Internal server error", body = smista_core::api::ApiError),
        )
    )
)]
pub(crate) async fn get_traces(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
    Query(params): Query<TraceQueryParams>,
) -> ApiResult<TraceResponse> {
    let Ok(session_id) = Uuid::parse_str(&session_id) else {
        return Err(WebError::from_code(
            ApiErrorCode::InvalidSessionId,
            "Invalid session id.",
        ));
    };
    let tracer = Tracer::new(state.database.clone(), session_id, user.user_id);

    let pagination = Pagination {
        limit: params.limit.unwrap_or(Pagination::DEFAULT_LIMIT),
        offset: params.offset.unwrap_or_default(),
    };
    match tracer.traces(pagination).await {
        Ok(Some(trace)) => Ok((StatusCode::OK, Json(TraceResponse { trace }))),
        Ok(None) => Err(WebError::from_code(
            ApiErrorCode::SessionNotFound,
            "Session not found.",
        )),
        Err(err) => {
            tracing::error!("Failed to fetch traces for session {}: {}", session_id, err);
            Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "Failed to fetch traces.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use smista_core::intent::TaskIntent;
    use smista_core::message::MessageRole;
    use smista_core::model::Provider;
    use smista_core::trace::MessagePayload;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::SurrealDatabase;
    use smista_storage::entity::{Session, Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::{ContentEnvelope, SecretContent};
    use uuid::Uuid;

    use crate::session::Sessions;
    use crate::trace::{SerializedPayload, TraceContext, Tracer};
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
        let session_id = Uuid::now_v7();
        Sessions::new(db.clone(), user_id)
            .create(Session::new(
                session_id,
                user_id,
                Some(title.to_string()),
                key_id,
            ))
            .await
            .expect("failed to create session");
        session_id
    }

    fn context() -> TraceContext {
        TraceContext {
            task_type: TaskIntent::Edit,
            provider: Provider::Anthropic,
            model: "claude-sonnet".to_string(),
            matched_rule: Some("edit -> claude".to_string()),
        }
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

    /// Records a single message event carrying `content` for the session.
    async fn record_message(
        db: &SurrealDatabase,
        user_id: Uuid,
        session_id: Uuid,
        content: SecretContent,
    ) {
        Tracer::new(db.clone(), session_id, user_id)
            .record_message(context(), content)
            .await
            .expect("failed to record message");
    }

    /// Wraps a message payload carrying `model` as plaintext content.
    fn plaintext_message(model: &str) -> SecretContent {
        let payload = SerializedPayload::message(MessagePayload {
            role: MessageRole::User,
            provider: Provider::Anthropic,
            model: model.to_string(),
        })
        .expect("failed to serialize message payload");
        SecretContent::plaintext(payload.into_string())
    }

    #[tokio::test]
    async fn should_return_the_sessions_trace_oldest_first() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Refactor auth middleware", None).await;
        record_message(&db, user_id, session_id, plaintext_message("claude-0")).await;
        record_message(&db, user_id, session_id, plaintext_message("claude-1")).await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/traces"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let trace = &body["trace"];
        assert_eq!(trace["session_id"], session_id.to_string());
        let events = trace["events"].as_array().expect("events missing");
        assert_eq!(events.len(), 2);
        // Oldest first: the first recorded event leads.
        assert_eq!(events[0]["event_type"], "message");
        assert_eq!(events[0]["task_type"], "edit");
        assert_eq!(events[0]["provider"], "anthropic");
        assert_eq!(events[0]["payload"]["plaintext"]["model"], "claude-0");
        assert_eq!(events[1]["payload"]["plaintext"]["model"], "claude-1");
    }

    #[tokio::test]
    async fn should_return_empty_events_for_a_window_past_the_end() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Fresh", None).await;
        record_message(&db, user_id, session_id, plaintext_message("claude")).await;

        let (status, body) = send(
            router,
            get_with_token(
                &format!("/api/v1/sessions/{session_id}/traces?limit=50&offset=10"),
                &token,
            ),
        )
        .await;

        // A session that resolves but has no events in the window answers 200
        // with an empty list, not 404.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["trace"]["session_id"], session_id.to_string());
        assert_eq!(body["trace"]["events"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn should_honor_the_limit_and_offset_query_params() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Paged", None).await;
        for index in 0..3u32 {
            record_message(
                &db,
                user_id,
                session_id,
                plaintext_message(&format!("claude-{index}")),
            )
            .await;
        }

        let (status, body) = send(
            router.clone(),
            get_with_token(
                &format!("/api/v1/sessions/{session_id}/traces?limit=2&offset=0"),
                &token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let first = body["trace"]["events"].as_array().expect("events missing");
        assert_eq!(first.len(), 2);
        assert_eq!(first[0]["payload"]["plaintext"]["model"], "claude-0");
        assert_eq!(first[1]["payload"]["plaintext"]["model"], "claude-1");

        let (status, body) = send(
            router,
            get_with_token(
                &format!("/api/v1/sessions/{session_id}/traces?limit=2&offset=2"),
                &token,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let second = body["trace"]["events"].as_array().expect("events missing");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0]["payload"]["plaintext"]["model"], "claude-2");
    }

    #[tokio::test]
    async fn should_return_a_sealed_payload_for_an_encrypted_session() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "secret", Some("kf_ab12".to_string())).await;
        record_message(&db, user_id, session_id, SecretContent::from(envelope())).await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/traces"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let event = &body["trace"]["events"][0];
        // The router holds no key, so it returns the sealed envelope verbatim.
        assert!(event["payload"].get("plaintext").is_none());
        assert_eq!(event["payload"]["encrypted"]["key_id"], "kf_ab12");
        assert_eq!(
            event["payload"]["encrypted"]["ciphertext"],
            "Y2lwaGVydGV4dA"
        );
    }

    #[tokio::test]
    async fn should_return_not_found_for_an_unknown_session() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            get_with_token(
                &format!("/api/v1/sessions/{}/traces", Uuid::now_v7()),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_treat_a_trace_of_an_archived_session_as_not_found() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "archived", None).await;
        record_message(&db, user_id, session_id, plaintext_message("claude")).await;
        Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session")
            .archive()
            .await
            .expect("failed to archive session");

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/traces"), &token),
        )
        .await;

        // An archived session is treated as gone, so its trace is never returned.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_treat_a_trace_of_another_users_session_as_not_found() {
        let (router, token, _user_id, db) = authenticated_router_with_database().await;

        // A second user owns a session the authenticated caller must not see.
        let other_id = Uuid::now_v7();
        db.create_user(User {
            id: RecordId::new(User::name(), other_id.to_string()),
            api_key_hash: "hash".to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            disabled_at: None,
        })
        .await
        .expect("failed to create other user");
        let session_id = create_session(&db, other_id, "not yours", None).await;
        record_message(&db, other_id, session_id, plaintext_message("claude")).await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/traces"), &token),
        )
        .await;

        // Reported as not found, never as forbidden, so existence stays private.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_reject_a_malformed_session_id() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            get_with_token("/api/v1/sessions/not-a-uuid/traces", &token),
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
            get(&format!("/api/v1/sessions/{}/traces", Uuid::now_v7())),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}

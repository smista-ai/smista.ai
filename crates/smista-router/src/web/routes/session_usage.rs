//! `GET /api/v1/sessions/{session_id}/usage` — report session usage.
//!
//! Returns the session total plus per-model and per-task-type breakdowns as a
//! [`SessionUsageResponse`](smista_core::api::SessionUsageResponse).
//!
//! Protected endpoint: the [`authenticate`](crate::web) middleware resolves the
//! owner from the bearer token, and the lookup is scoped to that user. An
//! archived session, or one owned by another user, is treated as absent and
//! never disclosed, so it yields the same `404` as an unknown id.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, SessionUsageResponse};
use uuid::Uuid;

use crate::usage::SessionUsage;
use crate::web::error::WebError;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `GET /api/v1/sessions/{session_id}/usage`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        get,
        path = "/api/v1/sessions/{session_id}/usage",
        operation_id = "getSessionUsage",
        tag = "usage",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        responses(
            (status = 200, description = "Session usage totals and breakdowns", body = smista_core::api::SessionUsageResponse),
            (status = 400, description = "Invalid session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found", body = smista_core::api::ApiError),
            (status = 500, description = "Internal server error", body = smista_core::api::ApiError),
        )
    )
)]
pub(crate) async fn session_usage(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
) -> ApiResult<SessionUsageResponse> {
    let Ok(session_id) = Uuid::parse_str(&session_id) else {
        return Err(WebError::from_code(
            ApiErrorCode::InvalidSessionId,
            "Invalid session id.",
        ));
    };

    let usage = SessionUsage::new(state.database.clone(), session_id, user.user_id);
    match usage.usage().await {
        Ok(Some(response)) => Ok((StatusCode::OK, Json(response))),
        Ok(None) => Err(WebError::from_code(
            ApiErrorCode::SessionNotFound,
            "Session not found.",
        )),
        Err(err) => {
            tracing::error!("Failed to aggregate usage for session {session_id}: {err}");
            Err(WebError::from_code(
                ApiErrorCode::InternalError,
                "Failed to fetch usage.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use smista_core::intent::TaskIntent;
    use smista_core::model::Provider;
    use smista_core::trace::CostPayload;
    use smista_storage::database::Database as _;
    use smista_storage::database::surreal::SurrealDatabase;
    use smista_storage::entity::{Session, Table as _, User};
    use smista_storage::surrealdb::RecordId;
    use smista_storage::types::SecretContent;
    use uuid::Uuid;

    use crate::session::Sessions;
    use crate::trace::{SerializedPayload, TraceContext, Tracer};
    use crate::web::test_support::{
        authenticated_router, authenticated_router_with_database, get, get_with_token, send,
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

    /// Builds a cost payload pricing `input`/`output` tokens at `cost`.
    fn cost_payload(
        provider: Provider,
        model: &str,
        input: u64,
        output: u64,
        cost: &str,
    ) -> CostPayload {
        CostPayload {
            provider,
            model: model.to_string(),
            input_tokens: input,
            output_tokens: output,
            cost: Some(cost.to_string()),
        }
    }

    /// Records a cost event for the session, routed under `task_type` and the
    /// payload's own provider and model.
    async fn record_cost(
        db: &SurrealDatabase,
        user_id: Uuid,
        session_id: Uuid,
        task_type: TaskIntent,
        cost: CostPayload,
    ) {
        let context = TraceContext {
            task_type,
            provider: cost.provider.clone(),
            model: cost.model.clone(),
            matched_rule: None,
        };
        let payload = SerializedPayload::cost(cost).expect("failed to serialize cost payload");
        Tracer::new(db.clone(), session_id, user_id)
            .record_cost(context, SecretContent::plaintext(payload.into_string()))
            .await
            .expect("failed to record cost");
    }

    #[tokio::test]
    async fn should_report_usage_across_models_and_task_types() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Refactor auth middleware").await;
        record_cost(
            &db,
            user_id,
            session_id,
            TaskIntent::Edit,
            cost_payload(Provider::OpenAI, "gpt-5.5-thinking", 8_000, 2_200, "0.31"),
        )
        .await;
        record_cost(
            &db,
            user_id,
            session_id,
            TaskIntent::Plan,
            cost_payload(Provider::Anthropic, "claude-sonnet", 4_000, 1_200, "0.18"),
        )
        .await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/usage"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total"]["input_tokens"], 12_000);
        assert_eq!(body["total"]["total_tokens"], 15_400);
        assert_eq!(body["total"]["estimated_cost"], "0.49");
        assert_eq!(body["total"]["currency"], "USD");
        let by_model = body["by_model"].as_array().expect("by_model missing");
        assert_eq!(by_model.len(), 2);
        assert_eq!(by_model[0]["provider"], "openai");
        assert_eq!(by_model[0]["model"], "gpt-5.5-thinking");
        assert_eq!(by_model[0]["request_count"], 1);
        assert_eq!(by_model[0]["input_tokens"], 8_000);
        let by_task = body["by_task_type"]
            .as_array()
            .expect("by_task_type missing");
        assert_eq!(by_task.len(), 2);
        assert_eq!(by_task[0]["task_type"], "edit");
        assert_eq!(by_task[1]["task_type"], "plan");
        assert_eq!(by_task[1]["estimated_cost"], "0.18");
    }

    #[tokio::test]
    async fn should_report_empty_usage_for_a_session_without_cost() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "Fresh").await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/usage"), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["by_model"], serde_json::json!([]));
        assert_eq!(body["by_task_type"], serde_json::json!([]));
        // Tokens never reported are omitted, not zeroed.
        assert!(body["total"].get("input_tokens").is_none());
    }

    #[tokio::test]
    async fn should_return_not_found_for_an_unknown_session() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            get_with_token(
                &format!("/api/v1/sessions/{}/usage", Uuid::now_v7()),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_treat_another_users_session_as_not_found() {
        let (router, token, _user_id, db) = authenticated_router_with_database().await;
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
        let session_id = create_session(&db, other_id, "not yours").await;

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/usage"), &token),
        )
        .await;

        // Reported as not found, never as forbidden, so existence stays private.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_treat_an_archived_session_as_not_found() {
        let (router, token, user_id, db) = authenticated_router_with_database().await;
        let session_id = create_session(&db, user_id, "archived").await;
        Sessions::new(db.clone(), user_id)
            .open(session_id)
            .await
            .expect("failed to open session")
            .archive()
            .await
            .expect("failed to archive session");

        let (status, body) = send(
            router,
            get_with_token(&format!("/api/v1/sessions/{session_id}/usage"), &token),
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
            get_with_token("/api/v1/sessions/not-a-uuid/usage", &token),
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
            get(&format!("/api/v1/sessions/{}/usage", Uuid::now_v7())),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }
}

//! `POST /api/v1/sessions/{session_id}/continue` — advance an in-flight run.
//!
//! Takes a [`ContinueRequest`](smista_core::api::ContinueRequest) — tool
//! results, approval decisions, decrypted or sealed records, queued user input
//! or an interrupt — routes it deterministically through the [`Orchestrator`],
//! advances the run and returns the next
//! [`TurnResponse`](smista_core::api::TurnResponse) with the same shape the
//! execute endpoint returns. It replaces the standalone approval endpoint: a
//! tool's approval rides its result, and a decision with no tool to run rides
//! the approval decisions.
//!
//! The reply is buffered as a single `TurnResponse` (`application/json`) by
//! default, or streamed as [`TurnEvent`](smista_core::api::TurnEvent)
//! Server-Sent Events when the client asks via `Accept: text/event-stream`. As
//! with execute, a streamed request is driven from the resumed turn's live model
//! output, with the terminal events appended once it finishes.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, ContinueRequest};
use uuid::Uuid;

use crate::orchestrator::Orchestrator;
use crate::router::resolver::Resolver;
use crate::web::error::WebError;
use crate::web::middleware::RequestCredentials;
use crate::web::streaming::{stream_turn, wants_event_stream};
use crate::web::{AppState, AuthenticatedUser};

/// Handles `POST /api/v1/sessions/{session_id}/continue`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/continue",
        operation_id = "continueTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ContinueRequest,
        responses(
            (
                status = 200,
                description = "Turn completed or awaiting input. Buffered as a single `TurnResponse` (`application/json`) or, when the client sends `Accept: text/event-stream`, streamed as Server-Sent Events of `TurnEvent` whose terminal `turn_end` carries the `TurnResponse`.",
                content(
                    (smista_core::api::TurnResponse = "application/json"),
                    (smista_core::api::TurnEvent = "text/event-stream"),
                )
            ),
            (status = 400, description = "Malformed session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 409, description = "A turn is already in flight for the run", body = smista_core::api::ApiError),
            (status = 422, description = "Routing rejected the request, no run is in progress, or the continuation does not answer the current pause", body = smista_core::api::ApiError),
            (status = 503, description = "Provider credentials missing or fallbacks exhausted", body = smista_core::api::ApiError),
            (status = 502, description = "Provider error", body = smista_core::api::ApiError),
            (status = 504, description = "Provider timed out", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn continue_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    credentials: Option<Extension<RequestCredentials>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
    Json(request): Json<ContinueRequest>,
) -> Response {
    // The wire format is governed by content negotiation, exactly as execute: the
    // buffered JSON body is the default, and only an explicit `text/event-stream`
    // `Accept` asks for the Server-Sent Events stream.
    let streaming = wants_event_stream(&headers);

    let Ok(session_id) = Uuid::parse_str(&session_id) else {
        return WebError::from_code(ApiErrorCode::InvalidSessionId, "Invalid session id.")
            .into_response();
    };

    // Provider credentials, when supplied, travel as per-request headers lifted
    // into the request extensions by the credential middleware. They are used for
    // this one turn and never persisted; a request without them runs credential-free.
    let credentials = credentials
        .map(|Extension(credentials)| credentials.credentials())
        .unwrap_or_default();

    let orchestrator = Orchestrator::new(
        state.database.clone(),
        state.router.clone(),
        Arc::new(Resolver::default()),
    );

    if streaming {
        // As with execute, a streamed continuation is driven on a spawned task so
        // the resumed turn's deltas flush as they arrive.
        let user_id = user.user_id;
        return stream_turn(move |sink| async move {
            orchestrator
                .advance_streaming(user_id, session_id, request, credentials, Some(sink))
                .await
        });
    }

    match orchestrator
        .advance(user.user_id, session_id, request, credentials)
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => WebError::from(error).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode, header};
    use smista_core::api::{
        ApprovalDecision, ApprovalDecisionEntry, Attachments, ContinueRequest, ExecutePolicy,
        ExecuteRequest, LocalPreferences, TaskInput, ToolResult, Workspace,
    };
    use smista_core::policy::{
        ClassificationConfig, DefaultRoute, PermissionMode, PrivacyPolicy, RoutingPolicy,
        ToolsConfig,
    };
    use smista_core::usage::Usage;
    use smista_providers::api::{CompletionResponse, FinishReason, ToolCall};
    use uuid::Uuid;

    use crate::router::Router as SmistaRouter;
    use crate::web::test_support::{
        authenticated_router_with_database, authenticated_router_with_router, post,
        post_json_with_token, send, send_raw, send_status, sse_events,
    };

    /// Creates a session for the authenticated user and returns its id, so a
    /// continuation has a run to advance.
    async fn create_session(router: Router, token: &str) -> Uuid {
        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions",
                token,
                &serde_json::json!({ "title": "continue test" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        Uuid::parse_str(body["session"]["id"].as_str().expect("id missing")).expect("bad uuid")
    }

    /// An execute body that routes to the local mock model by default.
    fn execute_request() -> ExecuteRequest {
        ExecuteRequest {
            input: TaskInput {
                text: "hello".to_string(),
                command: None,
                explicit_model: None,
            },
            workspace: Workspace {
                root: std::path::PathBuf::from("/repo"),
                git_branch: None,
                git_diff: None,
                referenced_paths: Vec::new(),
                active_file: None,
            },
            policy: ExecutePolicy {
                version: 1,
                source: "merged".to_string(),
                classification: ClassificationConfig::default(),
                routing: RoutingPolicy {
                    rules: Vec::new(),
                    default: Some(DefaultRoute {
                        model: "ollama/mock-local".parse().expect("valid reference"),
                        fallbacks: Vec::new(),
                    }),
                },
                tools: ToolsConfig::default(),
                privacy: PrivacyPolicy::default(),
            },
            local_preferences: LocalPreferences {
                auto_apply: false,
                stream: false,
                local_only: false,
                no_network: false,
            },
            attachments: Attachments {
                files: Vec::new(),
                instructions: Vec::new(),
                invoked_skills: Vec::new(),
                available_skills: Vec::new(),
            },
        }
    }

    /// A completion that requests one tool with empty arguments.
    fn tool_call_response(name: &str) -> CompletionResponse {
        CompletionResponse {
            content: String::new(),
            tool_calls: vec![ToolCall {
                call_id: format!("{name}-1"),
                name: name.to_string(),
                arguments: serde_json::json!({}),
            }],
            usage: Usage::default(),
            finish_reason: FinishReason::ToolCalls,
        }
    }

    /// Builds a `POST /execute` request carrying `token` and the JSON `body`.
    fn execute_post(session_id: Uuid, token: &str, body: &ExecuteRequest) -> Request<Body> {
        post_json_with_token(
            &format!("/api/v1/sessions/{session_id}/execute"),
            token,
            &serde_json::to_value(body).expect("failed to serialize execute body"),
        )
    }

    /// Builds a `POST /continue` request carrying `token` and the JSON `body`.
    fn continue_post(session_id: Uuid, token: &str, body: &ContinueRequest) -> Request<Body> {
        post_json_with_token(
            &format!("/api/v1/sessions/{session_id}/continue"),
            token,
            &serde_json::to_value(body).expect("failed to serialize continue body"),
        )
    }

    /// Builds a `POST` request with an explicit `Accept` for streaming.
    fn streaming_post(uri: &str, token: &str, body: &serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "text/event-stream")
            .body(Body::from(
                serde_json::to_vec(body).expect("serialize body"),
            ))
            .expect("build request")
    }

    #[tokio::test]
    async fn should_stream_a_continuation_to_completion() {
        // The execute turn pauses on an allowed tool; the streamed continuation
        // feeds the result back and the resumed turn streams to completion.
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "read_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.policy.tools.set("read_file", PermissionMode::Allow);

        let (status, body) = send(router.clone(), execute_post(session_id, &token, &request)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "awaiting_tool");
        let call_id = body["data"]["tool_requests"][0]["call_id"]
            .as_str()
            .expect("call_id missing")
            .to_string();

        let continuation = ContinueRequest::ToolResults {
            results: vec![ToolResult {
                call_id,
                content: "file body".to_string(),
                is_error: false,
                decision: None,
            }],
            encrypted: Default::default(),
        };

        let (status, content_type, raw) = send_raw(
            router,
            streaming_post(
                &format!("/api/v1/sessions/{session_id}/continue"),
                &token,
                &serde_json::to_value(&continuation).expect("serialize continuation"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/event-stream"));
        let events = sse_events(&raw);
        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "completed");
    }

    #[tokio::test]
    async fn should_advance_with_a_tool_result_and_complete() {
        // A scripted mock asks for an allowed tool, so the execute turn pauses for
        // the client to run it; the continuation feeds the result back.
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "read_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.policy.tools.set("read_file", PermissionMode::Allow);

        let (status, body) = send(router.clone(), execute_post(session_id, &token, &request)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "awaiting_tool");
        let call_id = body["data"]["tool_requests"][0]["call_id"]
            .as_str()
            .expect("call_id missing")
            .to_string();

        let continuation = ContinueRequest::ToolResults {
            results: vec![ToolResult {
                call_id,
                content: "file body".to_string(),
                is_error: false,
                decision: None,
            }],
            encrypted: Default::default(),
        };

        let (status, body) = send(router, continue_post(session_id, &token, &continuation)).await;
        // The script is exhausted, so the resumed turn completes with a message.
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "completed");
    }

    #[tokio::test]
    async fn should_advance_with_an_approval_decision() {
        // A `plan` command drives the execute turn to a plan-approval pause; the
        // continuation answers it with a decision.
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "edit_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.input.command = Some(smista_core::intent::TaskIntent::Plan);

        let (status, body) = send(router.clone(), execute_post(session_id, &token, &request)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "awaiting_approval");
        let approval_id = body["data"]["approval"]["approval_id"]
            .as_str()
            .expect("approval_id missing")
            .to_string();

        let continuation = ContinueRequest::ApprovalDecisions {
            decisions: vec![ApprovalDecisionEntry {
                approval_id,
                decision: ApprovalDecision::Approved,
                reason: None,
            }],
            encrypted: Default::default(),
        };

        let (status, body) = send(router, continue_post(session_id, &token, &continuation)).await;
        // Accepting the plan advances the run; the reply carries the next outcome.
        assert_eq!(status, StatusCode::OK);
        assert!(body.get("status").is_some(), "reply names an outcome");
    }

    #[tokio::test]
    async fn should_abort_the_in_flight_turn_with_break() {
        // The execute turn pauses on an allowed tool; `break` aborts it and returns
        // the run to idle, keeping any partial output.
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "read_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.policy.tools.set("read_file", PermissionMode::Allow);

        let (status, body) = send(router.clone(), execute_post(session_id, &token, &request)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "awaiting_tool");

        let (status, body) = send(
            router,
            continue_post(session_id, &token, &ContinueRequest::Break),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "idle");
    }

    #[tokio::test]
    async fn should_reject_a_continuation_without_an_active_run() {
        // A fresh session has no run in progress, so a continuation is rejected.
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        let (status, body) = send(
            router,
            continue_post(session_id, &token, &ContinueRequest::Break),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "invalid_request");
    }

    #[tokio::test]
    async fn should_reject_a_malformed_session_id() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions/not-a-uuid/continue",
                &token,
                &serde_json::to_value(ContinueRequest::Break).expect("serialize break"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_session_id");
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = Uuid::now_v7();

        let status = send_status(
            router,
            post(&format!("/api/v1/sessions/{session_id}/continue")),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

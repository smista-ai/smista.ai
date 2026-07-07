//! `POST /api/v1/sessions/{session_id}/execute` — run a task.
//!
//! Takes an [`ExecuteRequest`](smista_core::api::ExecuteRequest), routes it
//! deterministically through the [`Orchestrator`], calls the selected model and
//! returns a [`TurnResponse`](smista_core::api::TurnResponse) with the routing
//! explanation. The reply is buffered as a single `TurnResponse`
//! (`application/json`) by default, or streamed as
//! [`TurnEvent`](smista_core::api::TurnEvent) Server-Sent Events when the client
//! asks via `Accept: text/event-stream`.
//!
//! A streamed request is driven from the model's live output: text and reasoning
//! deltas arrive as the model produces them, and the terminal events — usage,
//! tool-call activity and the closing `turn_end` carrying the full response — are
//! appended once the turn finishes. A model that cannot stream is replayed as the
//! same event shape, so the client only ever handles one stream.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, ExecuteRequest};
use uuid::Uuid;

use crate::orchestrator::Orchestrator;
use crate::router::resolver::Resolver;
use crate::web::error::WebError;
use crate::web::middleware::RequestCredentials;
use crate::web::streaming::{stream_turn, wants_event_stream};
use crate::web::{AppState, AuthenticatedUser};

/// Handles `POST /api/v1/sessions/{session_id}/execute`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/execute",
        operation_id = "executeTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ExecuteRequest,
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
            (status = 403, description = "An explicit model override is forbidden by policy", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found or no access to session", body = smista_core::api::ApiError),
            (status = 422, description = "Routing rejected the request", body = smista_core::api::ApiError),
            (status = 503, description = "Provider credentials missing or fallbacks exhausted", body = smista_core::api::ApiError),
            (status = 502, description = "Provider error", body = smista_core::api::ApiError),
            (status = 504, description = "Provider timed out", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn execute(
    State(state): State<AppState>,
    headers: HeaderMap,
    credentials: Option<Extension<RequestCredentials>>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(session_id): Path<String>,
    Json(request): Json<ExecuteRequest>,
) -> Response {
    // The wire format is governed by content negotiation: the buffered JSON body
    // is the default, and only an explicit `text/event-stream` `Accept` asks for
    // the Server-Sent Events stream. An absent or `*/*` `Accept` stays JSON.
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
        // A streamed turn is driven on a spawned task so deltas flush as the model
        // produces them; the shared helper appends the terminal events.
        let user_id = user.user_id;
        return stream_turn(move |sink| async move {
            orchestrator
                .execute_streaming(user_id, session_id, request, credentials, Some(sink))
                .await
        });
    }

    match orchestrator
        .execute(user.user_id, session_id, request, credentials)
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
        Attachments, ExecutePolicy, ExecuteRequest, LocalPreferences, TaskInput, Workspace,
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

    /// Creates a session for the authenticated user and returns its id, so an
    /// execute request has a run to drive.
    async fn create_session(router: Router, token: &str) -> Uuid {
        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions",
                token,
                &serde_json::json!({ "title": "execute test" }),
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

    /// Builds a `POST /execute` request for `session_id`, carrying `token`, the
    /// JSON `body`, and the given `accept` media type (when set).
    fn execute_post(
        session_id: &str,
        token: &str,
        body: &ExecuteRequest,
        accept: Option<&str>,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/sessions/{session_id}/execute"))
            .header("Authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(accept) = accept {
            builder = builder.header(header::ACCEPT, accept);
        }
        builder
            .body(Body::from(
                serde_json::to_vec(body).expect("failed to serialize request body"),
            ))
            .expect("failed to build request")
    }

    #[tokio::test]
    async fn should_complete_a_turn_and_return_json_by_default() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        // No `Accept` header: content negotiation defaults to the buffered JSON body.
        let (status, body) = send(
            router,
            execute_post(&session_id.to_string(), &token, &execute_request(), None),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "completed");
        assert_eq!(body["data"]["message"]["role"], "assistant");
        assert!(
            !body["data"]["message"]["content"]
                .as_str()
                .expect("content missing")
                .is_empty()
        );
        // A plaintext completed turn is terminal: no continuations are offered.
        assert!(body.get("allowed_continuations").is_none());
    }

    #[tokio::test]
    async fn should_default_to_json_for_a_wildcard_accept() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        // `*/*` is not an explicit `text/event-stream` request, so it stays JSON.
        let (status, content_type, body) = send_raw(
            router,
            execute_post(
                &session_id.to_string(),
                &token,
                &execute_request(),
                Some("*/*"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            content_type.starts_with("application/json"),
            "expected JSON, got {content_type}"
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("body was not JSON");
        assert_eq!(value["status"], "completed");
    }

    #[tokio::test]
    async fn should_stream_a_completed_turn_as_server_sent_events() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        let (status, content_type, body) = send_raw(
            router,
            execute_post(
                &session_id.to_string(),
                &token,
                &execute_request(),
                Some("text/event-stream"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(
            content_type.starts_with("text/event-stream"),
            "expected an event stream, got {content_type}"
        );

        let events = sse_events(&body);
        // Incremental text arrives as multiple deltas, reasoning is exposed, and
        // the stream ends with usage and a terminal completed `turn_end`.
        let text_deltas = events
            .iter()
            .filter(|event| event["type"] == "text_delta")
            .count();
        assert!(
            text_deltas >= 2,
            "expected incremental text, got {text_deltas}"
        );
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "reasoning_delta")
        );
        assert!(events.iter().any(|event| event["type"] == "usage"));
        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "completed");
    }

    #[tokio::test]
    async fn should_replay_a_non_streaming_model_over_the_stream() {
        let router = Arc::new(SmistaRouter::mock_non_streaming());
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let (status, content_type, body) = send_raw(
            router,
            execute_post(
                &session_id.to_string(),
                &token,
                &execute_request(),
                Some("text/event-stream"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/event-stream"));
        let events = sse_events(&body);
        // A non-streaming model is replayed as one text delta plus the terminal.
        let text_deltas: Vec<_> = events
            .iter()
            .filter(|event| event["type"] == "text_delta")
            .collect();
        assert_eq!(text_deltas.len(), 1);
        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "completed");
    }

    #[tokio::test]
    async fn should_stream_an_approval_pause() {
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "edit_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.input.command = Some(smista_core::intent::TaskIntent::Plan);

        let (status, content_type, body) = send_raw(
            router,
            execute_post(
                &session_id.to_string(),
                &token,
                &request,
                Some("text/event-stream"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/event-stream"));
        let events = sse_events(&body);
        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "awaiting_approval");
    }

    #[tokio::test]
    async fn should_end_a_failed_stream_with_an_error_turn_end() {
        let router = Arc::new(SmistaRouter::mock_stream_error());
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let (status, content_type, body) = send_raw(
            router,
            execute_post(
                &session_id.to_string(),
                &token,
                &execute_request(),
                Some("text/event-stream"),
            ),
        )
        .await;

        // The stream opened with 200; the failure rides the terminal event.
        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/event-stream"));
        let events = sse_events(&body);
        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "error");
    }

    #[tokio::test]
    async fn should_stream_an_awaiting_tool_pause_with_tool_events() {
        // A scripted mock asks for a tool the policy allows, so the turn pauses
        // for the client to run it instead of completing.
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "read_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.policy.tools.set("read_file", PermissionMode::Allow);

        let (status, content_type, body) = send_raw(
            router,
            execute_post(
                &session_id.to_string(),
                &token,
                &request,
                Some("text/event-stream"),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(content_type.starts_with("text/event-stream"));

        let events = sse_events(&body);
        assert!(
            events
                .iter()
                .any(|event| event["type"] == "tool_call_started" && event["name"] == "read_file")
        );
        let requested = events
            .iter()
            .find(|event| event["type"] == "tool_call_requested")
            .expect("no tool_call_requested event");
        assert_eq!(requested["name"], "read_file");
        assert_eq!(requested["requires_approval"], "allow");

        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "awaiting_tool");
        // The pause advertises the continuations the client may answer it with.
        let continuations = last["allowed_continuations"]
            .as_array()
            .expect("allowed_continuations missing");
        assert!(continuations.iter().any(|kind| kind == "tool_results"));
    }

    #[tokio::test]
    async fn should_pause_for_a_plan_approval_as_json() {
        // A `plan` command drives the turn to a plan-approval pause rather than a
        // completed message, the continuation the buffered reply reports.
        let router = Arc::new(SmistaRouter::mock_scripted(vec![tool_call_response(
            "edit_file",
        )]));
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let mut request = execute_request();
        request.input.command = Some(smista_core::intent::TaskIntent::Plan);

        let (status, body) = send(
            router,
            execute_post(&session_id.to_string(), &token, &request, None),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "awaiting_approval");
        assert_eq!(body["data"]["approval"]["kind"], "plan");
        let continuations = body["allowed_continuations"]
            .as_array()
            .expect("allowed_continuations missing");
        assert!(
            continuations
                .iter()
                .any(|kind| kind == "approval_decisions")
        );
    }

    #[tokio::test]
    async fn should_reject_a_malformed_session_id() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            execute_post("not-a-uuid", &token, &execute_request(), None),
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
            post(&format!("/api/v1/sessions/{session_id}/execute")),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

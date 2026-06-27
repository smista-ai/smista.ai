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
//! The orchestrator buffers a turn rather than streaming it token by token, so a
//! streamed request is answered by replaying the finished outcome as the same
//! event vocabulary a live stream uses, terminated by the `turn_end` event that
//! carries the full response.

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
use crate::web::streaming::{replay_events, sse_response, wants_event_stream};
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

    match orchestrator
        .execute(user.user_id, session_id, request, credentials)
        .await
    {
        Ok(response) if streaming => sse_response(&replay_events(response)),
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => WebError::from(error).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
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
    use tower::ServiceExt as _;
    use uuid::Uuid;

    use crate::router::Router as SmistaRouter;
    use crate::web::test_support::{
        authenticated_router_with_database, authenticated_router_with_router, post,
        post_json_with_token, send, send_status,
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

    /// Sends a request through the router and returns the status, the
    /// `Content-Type` header and the raw body, so a streaming response can be
    /// asserted on without parsing it as a single JSON value.
    async fn send_raw(router: Router, mut request: Request<Body>) -> (StatusCode, String, String) {
        request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
        )));
        let response = router
            .oneshot(request)
            .await
            .expect("router failed to handle the request");
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read the response body");
        let body = String::from_utf8(bytes.to_vec()).expect("response body was not UTF-8");
        (status, content_type, body)
    }

    /// Parses a Server-Sent Events body into its decoded `data:` events.
    fn sse_events(body: &str) -> Vec<serde_json::Value> {
        body.split("\n\n")
            .filter_map(|record| record.strip_prefix("data: "))
            .map(|json| serde_json::from_str(json).expect("event payload was not valid JSON"))
            .collect()
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
        // The completed turn replays its assistant text, its usage and a terminal
        // `turn_end` carrying the same outcome the buffered reply would.
        assert_eq!(events[0]["type"], "text_delta");
        assert!(
            !events[0]["delta"]
                .as_str()
                .expect("delta missing")
                .is_empty()
        );
        assert!(events.iter().any(|event| event["type"] == "usage"));
        let last = events.last().expect("stream had no events");
        assert_eq!(last["type"], "turn_end");
        assert_eq!(last["status"], "completed");
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

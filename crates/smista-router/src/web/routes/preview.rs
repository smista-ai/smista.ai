//! `POST /api/v1/sessions/{session_id}/preview` — preview a route.
//!
//! Takes the same body as `/execute` but never calls the model. Routes the
//! request deterministically through the [`Orchestrator`], which opens the
//! session, resolves the turn and returns a
//! [`PreviewResponse`](smista_core::api::PreviewResponse) with the chosen
//! provider/model, matched rule, included and excluded context, an estimated
//! cost range and the permissions the route would require. Providers may be
//! queried for their model catalogs, but no completion request is made and no
//! tokens are spent.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use smista_core::api::{ApiErrorCode, ExecuteRequest};
use uuid::Uuid;

use crate::orchestrator::Orchestrator;
use crate::router::resolver::Resolver;
use crate::web::error::WebError;
use crate::web::middleware::RequestCredentials;
use crate::web::routes::ApiResult;
use crate::web::{AppState, AuthenticatedUser};

/// Handles `POST /api/v1/sessions/{session_id}/preview`.
#[cfg_attr(
    feature = "openapi",
    utoipa::path(
        post,
        path = "/api/v1/sessions/{session_id}/preview",
        operation_id = "previewTurn",
        tag = "execution",
        security(("bearer" = [])),
        params(("session_id" = String, Path, description = "Session id")),
        request_body = smista_core::api::ExecuteRequest,
        responses(
            (status = 200, description = "Routing preview without model invocation", body = smista_core::api::PreviewResponse),
            (status = 400, description = "Malformed session id", body = smista_core::api::ApiError),
            (status = 401, description = "Missing or invalid token", body = smista_core::api::ApiError),
            (status = 403, description = "An explicit model override is forbidden by policy", body = smista_core::api::ApiError),
            (status = 404, description = "Session not found or no access to session", body = smista_core::api::ApiError),
            (status = 422, description = "Routing rejected the request", body = smista_core::api::ApiError),
            (status = 503, description = "Provider credentials missing or fallbacks exhausted", body = smista_core::api::ApiError)
        )
    )
)]
pub(crate) async fn preview(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    credentials: Option<Extension<RequestCredentials>>,
    Path(session_id): Path<String>,
    Json(request): Json<ExecuteRequest>,
) -> ApiResult<smista_core::api::PreviewResponse> {
    let Ok(session_id) = Uuid::parse_str(&session_id) else {
        return Err(WebError::from_code(
            ApiErrorCode::InvalidSessionId,
            "Invalid session id.",
        ));
    };

    // Provider credentials, when supplied, travel as per-request headers lifted
    // into the request extensions by the credential middleware. A preview reads
    // the catalog with them so model selection sees the models the turn would,
    // but never invokes the chosen model.
    let credentials = credentials
        .map(|Extension(credentials)| credentials.credentials())
        .unwrap_or_default();

    let orchestrator = Orchestrator::new(
        state.database.clone(),
        state.router.clone(),
        Arc::new(Resolver::default()),
    );

    match orchestrator
        .preview(user.user_id, session_id, request, credentials)
        .await
    {
        Ok(preview) => Ok((StatusCode::OK, Json(preview))),
        Err(error) => Err(WebError::from(error)),
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
    use uuid::Uuid;

    use crate::router::Router as SmistaRouter;
    use crate::web::test_support::{
        authenticated_router_with_database, authenticated_router_with_router, post,
        post_json_with_token, send, send_status,
    };

    /// Creates a session for the authenticated user and returns its id, so a
    /// preview request has a session to resolve against.
    async fn create_session(router: Router, token: &str) -> Uuid {
        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions",
                token,
                &serde_json::json!({ "title": "preview test" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        Uuid::parse_str(body["session"]["id"].as_str().expect("id missing")).expect("bad uuid")
    }

    /// A preview body that routes to the local mock model by default.
    fn preview_request() -> ExecuteRequest {
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

    /// Builds a `POST /preview` request for `session_id`, carrying `token` and
    /// the JSON `body`.
    fn preview_post(session_id: Uuid, token: &str, body: &ExecuteRequest) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/v1/sessions/{session_id}/preview"))
            .header("Authorization", format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(body).expect("failed to serialize request body"),
            ))
            .expect("failed to build request")
    }

    #[tokio::test]
    async fn should_preview_the_default_route() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        let (status, body) =
            send(router, preview_post(session_id, &token, &preview_request())).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["routing"]["provider"], "ollama");
        assert_eq!(body["routing"]["model"], "mock-local");
        // The default route names no rule, and the local mock is unpriced.
        assert!(body["routing"].get("matched_rule").is_none());
        assert_eq!(body["estimated_cost"]["min"], "0");
        assert_eq!(body["estimated_cost"]["max"], "0");
        assert_eq!(body["estimated_cost"]["currency"], "USD");
    }

    #[tokio::test]
    async fn should_preview_a_matched_rule_and_its_permissions() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        // A rule that matches an `edit` command, names itself, and tightens a
        // tool permission the preview must report.
        let mut request = preview_request();
        request.input.command = Some(smista_core::intent::TaskIntent::Edit);
        request
            .policy
            .tools
            .set("file_write", PermissionMode::Allow);
        request.policy.routing = serde_json::from_value(serde_json::json!({
            "default": { "model": "ollama/mock-local" },
            "rules": [{
                "name": "edits ask before writing",
                "intent": "edit",
                "model": "ollama/mock-local",
                "required_permissions": { "permissions": { "file_write": "ask" } },
            }],
        }))
        .expect("valid routing policy");

        let (status, body) = send(router, preview_post(session_id, &token, &request)).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["routing"]["intent"], "edit");
        assert_eq!(body["routing"]["matched_rule"], "edits ask before writing");
        let permissions = body["required_permissions"]
            .as_array()
            .expect("required_permissions missing");
        assert!(permissions.iter().any(|permission| {
            permission["permission"] == "file_write" && permission["mode"] == "ask"
        }));
    }

    #[tokio::test]
    async fn should_be_deterministic_across_calls() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;

        let (first_status, first) = send(
            router.clone(),
            preview_post(session_id, &token, &preview_request()),
        )
        .await;
        let (second_status, second) =
            send(router, preview_post(session_id, &token, &preview_request())).await;

        assert_eq!(first_status, StatusCode::OK);
        assert_eq!(second_status, StatusCode::OK);
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn should_use_provider_credentials_when_selecting_a_model() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        let session_id = create_session(router.clone(), &token).await;
        let mut request = preview_request();
        request.policy.routing.default = Some(DefaultRoute {
            model: "openai/mock-remote".parse().expect("valid reference"),
            fallbacks: vec!["ollama/mock-local".parse().expect("valid reference")],
        });

        let (missing_status, missing) =
            send(router.clone(), preview_post(session_id, &token, &request)).await;

        let mut credentialed = preview_post(session_id, &token, &request);
        credentialed.headers_mut().insert(
            header::HeaderName::from_static("x-smista-provider-openai-api-key"),
            header::HeaderValue::from_static("sk-openai"),
        );
        let (credentialed_status, credentialed) = send(router, credentialed).await;

        assert_eq!(missing_status, StatusCode::OK);
        assert_eq!(missing["routing"]["provider"], "ollama");
        assert_eq!(missing["routing"]["model"], "mock-local");
        assert_eq!(credentialed_status, StatusCode::OK);
        assert_eq!(credentialed["routing"]["provider"], "openai");
        assert_eq!(credentialed["routing"]["model"], "mock-remote");
    }

    #[tokio::test]
    async fn should_not_call_the_provider() {
        // A router whose every model call errors: a preview that touched the
        // provider would surface that error, but a preview never calls it.
        let router = Arc::new(SmistaRouter::mock_stream_error());
        let (router, token, _user_id, _db) = authenticated_router_with_router(router).await;
        let session_id = create_session(router.clone(), &token).await;

        let (status, body) =
            send(router, preview_post(session_id, &token, &preview_request())).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["routing"]["model"], "mock-local");
    }

    #[tokio::test]
    async fn should_reject_an_unknown_session() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;
        // A well-formed id that belongs to no session of this user is reported
        // as not found, the same as another user's session.
        let unknown = Uuid::now_v7();

        let (status, body) = send(router, preview_post(unknown, &token, &preview_request())).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "session_not_found");
    }

    #[tokio::test]
    async fn should_reject_a_malformed_session_id() {
        let (router, token, _user_id, _db) = authenticated_router_with_database().await;

        let (status, body) = send(
            router,
            post_json_with_token(
                "/api/v1/sessions/not-a-uuid/preview",
                &token,
                &serde_json::to_value(preview_request()).expect("serialize request"),
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
            post(&format!("/api/v1/sessions/{session_id}/preview")),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

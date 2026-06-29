//! A mock `smista-router` web server for testing concrete [`Client`]s.
//!
//! [`MockRouter`] starts a real local HTTP server (backed by [`wiremock`]) that
//! answers every router endpoint with a canned, schema-correct success body, so
//! an HTTP-backed [`Client`](crate::Client) implementation can be driven against
//! a faithful stand-in for the router without a live service. It exists only
//! under `cfg(test)`: it is test scaffolding for this crate, never shipped.
//!
//! # Defaults and overrides
//!
//! [`MockRouter::start`] mounts the happy-path response for all endpoints. To
//! exercise an error path, a streamed turn, or any other non-default reply, use
//! [`MockRouter::builder`] and [`respond`](MockRouterBuilder::respond) to replace
//! a single [`Endpoint`]'s response, leaving the rest at their defaults. The
//! [`api_error`] and [`sse`] helpers build the two most common overrides. For a
//! case the builder cannot express, [`MockRouter::server`] exposes the underlying
//! [`MockServer`] to mount bespoke [`Mock`]s.
//!
//! The canned success bodies live in [`defaults`]; a test asserting on an exact
//! value can call the matching function to obtain the value it should expect.

pub(crate) mod defaults;

use std::collections::HashMap;

use smista_core::api::{ApiError, ApiErrorBody, ApiErrorCode, TurnEvent};
use url::Url;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// HTTP status returned by a successful creation, per the router.
const CREATED: u16 = 201;
/// HTTP status returned by a successful read or update, per the router.
const OK: u16 = 200;

/// One router endpoint the [`MockRouter`] serves.
///
/// Each variant maps to exactly one method-and-path the router exposes. The
/// streamed `execute`/`continue` variants share their route with the buffered
/// ones — the router distinguishes them by the `Accept` header, not the path —
/// so they are not separate endpoints here; override [`Execute`](Self::Execute)
/// or [`ContinueRun`](Self::ContinueRun) with an [`sse`] response to stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Endpoint {
    /// `GET /status`.
    Status,
    /// `POST /api/v1/auth/bootstrap`.
    Bootstrap,
    /// `POST /api/v1/auth/sign-in`.
    SignIn,
    /// `POST /api/v1/auth/sign-out`.
    SignOut,
    /// `GET /api/v1/auth/me`.
    Me,
    /// `POST /api/v1/sessions`.
    CreateSession,
    /// `GET /api/v1/sessions`.
    ListSessions,
    /// `GET /api/v1/sessions/{id}`.
    GetSession,
    /// `PUT /api/v1/sessions/{id}`.
    UpdateSession,
    /// `DELETE /api/v1/sessions/{id}`.
    DeleteSession,
    /// `POST /api/v1/sessions/{id}/execute`.
    Execute,
    /// `POST /api/v1/sessions/{id}/continue`.
    ContinueRun,
    /// `POST /api/v1/sessions/{id}/preview`.
    Preview,
    /// `GET /api/v1/sessions/{id}/traces`.
    GetTraces,
    /// `GET /api/v1/llm/providers`.
    ListProviders,
    /// `GET /api/v1/llm/models`.
    ListModels,
    /// `GET /api/v1/sessions/{id}/usage`.
    SessionUsage,
}

/// How an [`Endpoint`]'s path is matched against an incoming request.
enum PathKind {
    /// Match the request path exactly.
    Exact(&'static str),
    /// Match the request path against an anchored regular expression, so a
    /// session id (or any single path segment) stands in for `{id}`.
    Regex(&'static str),
}

impl Endpoint {
    /// Every endpoint, mounted by the [`MockRouter`] in turn.
    const ALL: [Self; 17] = [
        Self::Status,
        Self::Bootstrap,
        Self::SignIn,
        Self::SignOut,
        Self::Me,
        Self::CreateSession,
        Self::ListSessions,
        Self::GetSession,
        Self::UpdateSession,
        Self::DeleteSession,
        Self::Execute,
        Self::ContinueRun,
        Self::Preview,
        Self::GetTraces,
        Self::ListProviders,
        Self::ListModels,
        Self::SessionUsage,
    ];

    /// The HTTP method this endpoint answers.
    fn http_method(self) -> &'static str {
        match self {
            Self::Status
            | Self::Me
            | Self::ListSessions
            | Self::GetSession
            | Self::GetTraces
            | Self::ListProviders
            | Self::ListModels
            | Self::SessionUsage => "GET",
            Self::Bootstrap
            | Self::SignIn
            | Self::SignOut
            | Self::CreateSession
            | Self::Execute
            | Self::ContinueRun
            | Self::Preview => "POST",
            Self::UpdateSession => "PUT",
            Self::DeleteSession => "DELETE",
        }
    }

    /// How this endpoint's path is matched. Routes carrying a `{id}` segment use
    /// an anchored regex so the segment is not confused with a sub-path such as
    /// `/traces` or `/usage`.
    fn path_kind(self) -> PathKind {
        match self {
            Self::Status => PathKind::Exact("/status"),
            Self::Bootstrap => PathKind::Exact("/api/v1/auth/bootstrap"),
            Self::SignIn => PathKind::Exact("/api/v1/auth/sign-in"),
            Self::SignOut => PathKind::Exact("/api/v1/auth/sign-out"),
            Self::Me => PathKind::Exact("/api/v1/auth/me"),
            Self::CreateSession | Self::ListSessions => PathKind::Exact("/api/v1/sessions"),
            Self::GetSession | Self::UpdateSession | Self::DeleteSession => {
                PathKind::Regex(r"^/api/v1/sessions/[^/]+$")
            }
            Self::Execute => PathKind::Regex(r"^/api/v1/sessions/[^/]+/execute$"),
            Self::ContinueRun => PathKind::Regex(r"^/api/v1/sessions/[^/]+/continue$"),
            Self::Preview => PathKind::Regex(r"^/api/v1/sessions/[^/]+/preview$"),
            Self::GetTraces => PathKind::Regex(r"^/api/v1/sessions/[^/]+/traces$"),
            Self::ListProviders => PathKind::Exact("/api/v1/llm/providers"),
            Self::ListModels => PathKind::Exact("/api/v1/llm/models"),
            Self::SessionUsage => PathKind::Regex(r"^/api/v1/sessions/[^/]+/usage$"),
        }
    }

    /// The canned happy-path response this endpoint serves by default, with the
    /// same status code the router returns.
    fn default_response(self) -> ResponseTemplate {
        match self {
            Self::Status => ResponseTemplate::new(OK).set_body_json(defaults::status()),
            Self::Bootstrap => ResponseTemplate::new(CREATED).set_body_json(defaults::bootstrap()),
            Self::SignIn => ResponseTemplate::new(OK).set_body_json(defaults::sign_in()),
            Self::SignOut => ResponseTemplate::new(OK).set_body_json(defaults::sign_out()),
            Self::Me => ResponseTemplate::new(OK).set_body_json(defaults::me()),
            Self::CreateSession => {
                ResponseTemplate::new(CREATED).set_body_json(defaults::create_session())
            }
            Self::ListSessions => {
                ResponseTemplate::new(OK).set_body_json(defaults::list_sessions())
            }
            Self::GetSession => ResponseTemplate::new(OK).set_body_json(defaults::get_session()),
            Self::UpdateSession => {
                ResponseTemplate::new(OK).set_body_json(defaults::update_session())
            }
            Self::DeleteSession => {
                ResponseTemplate::new(OK).set_body_json(defaults::delete_session())
            }
            Self::Execute | Self::ContinueRun => {
                ResponseTemplate::new(OK).set_body_json(defaults::turn())
            }
            Self::Preview => ResponseTemplate::new(OK).set_body_json(defaults::preview()),
            Self::GetTraces => ResponseTemplate::new(OK).set_body_json(defaults::traces()),
            Self::ListProviders => {
                ResponseTemplate::new(OK).set_body_json(defaults::list_providers())
            }
            Self::ListModels => ResponseTemplate::new(OK).set_body_json(defaults::list_models()),
            Self::SessionUsage => {
                ResponseTemplate::new(OK).set_body_json(defaults::session_usage())
            }
        }
    }

    /// Builds the [`Mock`] that serves `response` for this endpoint's
    /// method-and-path.
    fn mock(self, response: ResponseTemplate) -> Mock {
        let builder = Mock::given(method(self.http_method()));
        let builder = match self.path_kind() {
            PathKind::Exact(value) => builder.and(path(value)),
            PathKind::Regex(value) => builder.and(path_regex(value)),
        };
        builder.respond_with(response)
    }
}

/// A running mock of the `smista-router` HTTP API.
///
/// Start one with [`MockRouter::start`] for all-default behavior, or build a
/// customized one with [`MockRouter::builder`]. Point a client at
/// [`base_url`](Self::base_url) (or [`uri`](Self::uri)); the server stops when the
/// value is dropped.
pub(crate) struct MockRouter {
    /// The underlying mock HTTP server.
    server: MockServer,
}

impl MockRouter {
    /// Starts a mock router serving every endpoint's default response.
    pub(crate) async fn start() -> Self {
        Self::builder().start().await
    }

    /// Begins building a mock router whose responses can be overridden per
    /// endpoint before it starts.
    pub(crate) fn builder() -> MockRouterBuilder {
        MockRouterBuilder::default()
    }

    /// The server's base URL, as a [`RouterClientConfig`] expects it: scheme and
    /// host only, with no `/api/v1` suffix.
    ///
    /// [`RouterClientConfig`]: crate::RouterClientConfig
    pub(crate) fn base_url(&self) -> Url {
        Url::parse(&self.server.uri()).expect("wiremock yields a valid base URL")
    }

    /// The server's base URL as a string, for callers that build request URLs by
    /// hand.
    pub(crate) fn uri(&self) -> String {
        self.server.uri()
    }

    /// The underlying [`MockServer`], to mount additional bespoke mocks or to
    /// verify received requests.
    pub(crate) fn server(&self) -> &MockServer {
        &self.server
    }
}

/// Builder for a [`MockRouter`] with per-endpoint response overrides.
///
/// Created by [`MockRouter::builder`]. Any endpoint left without an override is
/// served its [default response](Endpoint::default_response).
#[derive(Default)]
pub(crate) struct MockRouterBuilder {
    /// Responses replacing the default for specific endpoints.
    overrides: HashMap<Endpoint, ResponseTemplate>,
}

impl MockRouterBuilder {
    /// Replaces the response served for `endpoint`. The last override for a given
    /// endpoint wins.
    pub(crate) fn respond(mut self, endpoint: Endpoint, response: ResponseTemplate) -> Self {
        self.overrides.insert(endpoint, response);
        self
    }

    /// Starts the server, mounting each endpoint's override if set, else its
    /// default response.
    pub(crate) async fn start(mut self) -> MockRouter {
        let server = MockServer::start().await;
        for endpoint in Endpoint::ALL {
            let response = self
                .overrides
                .remove(&endpoint)
                .unwrap_or_else(|| endpoint.default_response());
            endpoint.mock(response).mount(&server).await;
        }
        MockRouter { server }
    }
}

/// Builds an error response carrying the router's structured [`ApiError`] body.
///
/// The HTTP status and wire `code` are taken from `code`, so the reply matches
/// what the router would send for that failure.
pub(crate) fn api_error(code: ApiErrorCode, message: &str) -> ResponseTemplate {
    let body = ApiError {
        error: ApiErrorBody {
            code: code.as_str().to_owned(),
            message: message.to_owned(),
            details: None,
        },
    };
    ResponseTemplate::new(code.status().as_u16()).set_body_json(body)
}

/// Builds a `text/event-stream` response replaying `events` in the router's
/// Server-Sent Events framing: one `data: <json>` record per event, each
/// terminated by a blank line.
pub(crate) fn sse(events: &[TurnEvent]) -> ResponseTemplate {
    let body: String = events
        .iter()
        .map(|event| {
            let json = serde_json::to_string(event).expect("a turn event serializes");
            format!("data: {json}\n\n")
        })
        .collect();
    ResponseTemplate::new(OK).set_body_raw(body.into_bytes(), "text/event-stream")
}

#[cfg(test)]
mod tests {
    use smista_core::api::{
        CreateSessionResponse, GetSessionResponse, StatusResponse, TurnResponse,
    };
    use uuid::Uuid;

    use super::*;

    /// Issues a GET against the running mock and decodes the JSON body.
    async fn get_json<T>(router: &MockRouter, path: &str) -> (u16, T)
    where
        T: serde::de::DeserializeOwned,
    {
        let response = reqwest::Client::new()
            .get(format!("{}{path}", router.uri()))
            .send()
            .await
            .expect("the request reaches the mock");
        let status = response.status().as_u16();
        let body = response.json().await.expect("the body decodes");
        (status, body)
    }

    #[tokio::test]
    async fn should_serve_the_default_status_body() {
        let router = MockRouter::start().await;
        let (status, body) = get_json::<StatusResponse>(&router, "/status").await;
        assert_eq!(status, 200);
        assert_eq!(body, defaults::status());
    }

    #[tokio::test]
    async fn should_serve_creation_with_a_201_status() {
        let router = MockRouter::start().await;
        let response = reqwest::Client::new()
            .post(format!("{}/api/v1/sessions", router.uri()))
            .send()
            .await
            .expect("the request reaches the mock");
        assert_eq!(response.status().as_u16(), 201);
        let body: CreateSessionResponse = response.json().await.expect("the body decodes");
        assert_eq!(body, defaults::create_session());
    }

    #[tokio::test]
    async fn should_match_a_session_id_path_segment() {
        let router = MockRouter::start().await;
        let path = format!("/api/v1/sessions/{}", Uuid::nil());
        let (status, body) = get_json::<GetSessionResponse>(&router, &path).await;
        assert_eq!(status, 200);
        assert_eq!(body, defaults::get_session());
    }

    #[tokio::test]
    async fn should_distinguish_a_session_id_from_its_subpaths() {
        // `/sessions/{id}` and `/sessions/{id}/usage` must not be confused: the
        // anchored regex for the bare id must not also match the usage subpath.
        let router = MockRouter::start().await;
        let path = format!("/api/v1/sessions/{}/usage", Uuid::nil());
        let (status, _body) =
            get_json::<smista_core::api::SessionUsageResponse>(&router, &path).await;
        assert_eq!(status, 200);
    }

    #[tokio::test]
    async fn should_apply_a_per_endpoint_override() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::Status,
                api_error(ApiErrorCode::InternalError, "boom"),
            )
            .start()
            .await;
        let (status, body) = get_json::<ApiError>(&router, "/status").await;
        assert_eq!(status, 500);
        assert_eq!(body.error.code, "internal_error");
        assert_eq!(body.error.message, "boom");
    }

    #[tokio::test]
    async fn should_serve_a_streamed_turn_as_event_stream() {
        let events = [
            TurnEvent::TextDelta {
                delta: "Hello".to_owned(),
            },
            TurnEvent::TurnEnd(Box::new(defaults::turn())),
        ];
        let router = MockRouter::builder()
            .respond(Endpoint::Execute, sse(&events))
            .start()
            .await;
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/v1/sessions/{}/execute",
                router.uri(),
                Uuid::nil()
            ))
            .header("accept", "text/event-stream")
            .send()
            .await
            .expect("the request reaches the mock");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream"),
        );
        let body = response.text().await.expect("the body is readable");
        assert!(body.contains("data: {\"type\":\"text_delta\",\"delta\":\"Hello\"}\n\n"));
        assert!(body.contains("\"type\":\"turn_end\""));
    }

    #[tokio::test]
    async fn should_expose_a_base_url_for_a_client_config_and_record_requests() {
        let router = MockRouter::start().await;
        // `base_url` is shaped for a client config: scheme and host, no suffix.
        let config = crate::RouterClientConfig::new(router.base_url());
        assert_eq!(config.base_url, router.base_url());
        assert!(config.base_url.path() == "/");

        reqwest::Client::new()
            .get(format!("{}/status", router.uri()))
            .send()
            .await
            .expect("the request reaches the mock");

        // The underlying server records requests for later verification.
        let received = router
            .server()
            .received_requests()
            .await
            .expect("the mock records requests");
        assert!(!received.is_empty());
    }

    #[tokio::test]
    async fn should_decode_the_default_turn_for_execute() {
        let router = MockRouter::start().await;
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/v1/sessions/{}/execute",
                router.uri(),
                Uuid::nil()
            ))
            .send()
            .await
            .expect("the request reaches the mock");
        assert_eq!(response.status().as_u16(), 200);
        let body: TurnResponse = response.json().await.expect("the body decodes");
        assert_eq!(body, defaults::turn());
    }
}

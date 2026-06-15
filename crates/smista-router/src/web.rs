//! Web server for Smista Router.
//!
//! Hosts the HTTP JSON API consumed by the CLI, the Rust client and the
//! TypeScript SDK. Every documented endpoint is mounted under `/api/v1`, while
//! the public, unauthenticated `GET /status` health check sits at the root.
//!
//! The server is built around three pieces:
//!
//! - [`WebServer`], a long-lived service that binds the listener and serves the
//!   router until the shared [`CancellationToken`] is triggered.
//! - One request module per endpoint (for example [`status`] or [`execute`]),
//!   each owning a single handler. Endpoints not yet implemented return a
//!   `501 Not Implemented` [`error::WebError`] until their owning issue fills
//!   them in.
//! - Cross-cutting [`middleware`]: structured request logging with secrets kept
//!   out of the logs, and rejection of any credential passed as a query
//!   parameter.
//!
//! Domain errors are mapped to the wire format by
//! [`ApiErrorResponse`](smista_core::api::ApiErrorResponse) in `smista-core` and
//! rendered as JSON by [`error::WebError`].

mod bootstrap;
mod create_session;
mod delete_session;
mod error;
mod execute;
mod get_session;
mod get_trace;
mod latest_trace;
mod list_models;
mod list_providers;
mod list_sessions;
mod me;
mod middleware;
mod preview;
mod session_usage;
mod sign_in;
mod sign_out;
mod status;
mod stream;
mod submit_approval;
mod update_session;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use smista_storage::database::surreal::SurrealDatabase;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::RouterConfig;

/// Shared state threaded into every request handler.
///
/// Cloned once per request by `axum`, so each field is itself cheap to clone:
/// the configuration is shared behind an [`Arc`] and the database handle is a
/// shared connection.
#[derive(Debug, Clone)]
pub(crate) struct AppState {
    /// The validated router configuration.
    #[expect(
        dead_code,
        reason = "read by request handlers landing in follow-up issues (#133+)"
    )]
    pub(crate) config: Arc<RouterConfig>,
    /// The storage backend handle.
    #[expect(
        dead_code,
        reason = "read by request handlers landing in follow-up issues (#133+)"
    )]
    pub(crate) database: SurrealDatabase,
}

/// Dependencies required to construct a [`WebServer`].
#[derive(Debug)]
pub struct WebServerConfig {
    /// The validated router configuration; provides the bind host and port.
    pub config: RouterConfig,
    /// The storage backend handle.
    pub database: SurrealDatabase,
    /// Cancellation token that triggers a graceful shutdown when cancelled.
    pub exit: CancellationToken,
}

/// The router's HTTP server.
///
/// Construct it with [`WebServer::new`], then call [`WebServer::run`] to bind
/// the listener and start serving in a background task.
#[derive(Debug)]
pub struct WebServer {
    /// Shared handler state.
    state: AppState,
    /// Bind host.
    host: String,
    /// Bind port.
    port: u16,
    /// Cancellation token watched for graceful shutdown.
    exit: CancellationToken,
}

impl WebServer {
    /// Creates a new [`WebServer`] from its dependencies.
    pub fn new(config: WebServerConfig) -> Self {
        let host = config.config.host.clone();
        let port = config.config.port;
        Self {
            state: AppState {
                config: Arc::new(config.config),
                database: config.database,
            },
            host,
            port,
            exit: config.exit,
        }
    }

    /// Binds the listener and serves the API in a background task.
    ///
    /// Binding happens eagerly so a failure (for example, a port already in
    /// use) surfaces here rather than inside the spawned task. The returned
    /// [`JoinHandle`] resolves once the server has wound down after the
    /// cancellation token is triggered.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured host and port cannot be bound.
    pub async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let addr = format!("{host}:{port}", host = self.host, port = self.port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| anyhow::anyhow!("failed to bind web server to {addr}: {e}"))?;
        let local_addr = listener.local_addr()?;
        tracing::info!(server.address = %local_addr, "web server listening");

        let router = build_router(self.state);
        let exit = self.exit;
        let handle = tokio::spawn(async move {
            let shutdown = async move { exit.cancelled().await };
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(shutdown)
                .await
            {
                tracing::error!(%e, "web server stopped with an error");
            }
            tracing::info!("web server stopped");
        });

        Ok(handle)
    }
}

/// Builds the application router: the public health check, every `/api/v1`
/// endpoint and the cross-cutting middleware.
fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/bootstrap", post(bootstrap::bootstrap))
        .route("/auth/sign-in", post(sign_in::sign_in))
        .route("/auth/sign-out", post(sign_out::sign_out))
        .route("/auth/me", get(me::me))
        .route(
            "/sessions",
            post(create_session::create_session).get(list_sessions::list_sessions),
        )
        .route(
            "/sessions/{session_id}",
            get(get_session::get_session)
                .put(update_session::update_session)
                .delete(delete_session::delete_session),
        )
        .route("/sessions/{session_id}/execute", post(execute::execute))
        .route("/sessions/{session_id}/stream", post(stream::stream))
        .route("/sessions/{session_id}/preview", post(preview::preview))
        .route(
            "/sessions/{session_id}/approvals/{approval_id}",
            post(submit_approval::submit_approval),
        )
        .route(
            "/sessions/{session_id}/traces/latest",
            get(latest_trace::latest_trace),
        )
        .route(
            "/sessions/{session_id}/traces/{trace_id}",
            get(get_trace::get_trace),
        )
        .route(
            "/sessions/{session_id}/usage",
            get(session_usage::session_usage),
        )
        .route("/llm/providers", get(list_providers::list_providers))
        .route("/llm/models", get(list_models::list_models));

    Router::new()
        .route("/status", get(status::status))
        .nest("/api/v1", api)
        // Logging runs outermost so it observes the final status code, including
        // responses short-circuited by the credential guard below.
        .layer(axum::middleware::from_fn(middleware::log_requests))
        .layer(axum::middleware::from_fn(
            middleware::reject_query_credentials,
        ))
        .with_state(state)
}

/// Shared test scaffolding reused by every request module's unit tests.
///
/// Builds the application router over an in-memory database and the default
/// configuration, and sends requests through it in-process with
/// [`tower::ServiceExt::oneshot`], so tests need no network or open port.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use tower::ServiceExt as _;

    use super::{AppState, RouterConfig, build_router};

    /// Builds a fresh in-memory database for a test.
    pub(crate) async fn test_database() -> SurrealDatabase {
        SurrealDatabase::new(SurrealOptions {
            namespace: "test".to_string(),
            db: "test".to_string(),
            backend: SurrealBackend::Memory,
        })
        .await
        .expect("failed to initialize in-memory database")
    }

    /// Builds the application router backed by the default configuration and a
    /// fresh in-memory database.
    ///
    /// As the provider registry is added to [`AppState`] in a follow-up issue,
    /// mocked providers will be injected here so handler tests can exercise the
    /// provider and model endpoints without reaching a real backend.
    pub(crate) async fn test_router() -> Router {
        let state = AppState {
            config: Arc::new(RouterConfig::default()),
            database: test_database().await,
        };
        build_router(state)
    }

    /// Sends a request through the router and returns the status code together
    /// with the JSON body (or [`serde_json::Value::Null`] when the body is
    /// empty).
    pub(crate) async fn send(
        router: Router,
        request: Request<Body>,
    ) -> (StatusCode, serde_json::Value) {
        let response = router
            .oneshot(request)
            .await
            .expect("router failed to handle the request");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read the response body");
        let body = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("response body was not valid JSON")
        };
        (status, body)
    }

    /// Builds an empty `GET` request for `uri`.
    pub(crate) fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request")
    }
}

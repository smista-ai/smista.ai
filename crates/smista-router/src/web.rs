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

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use smista_storage::database::surreal::SurrealDatabase;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;

use crate::config::RouterConfig;
use crate::router::Router as SmistaRouter;

/// Shared state threaded into every request handler.
///
/// Cloned once per request by `axum`, so each field is itself cheap to clone:
/// the configuration is shared behind an [`Arc`] and the database handle is a
/// shared connection.
#[derive(Debug, Clone)]
pub(crate) struct AppState {
    /// The validated router configuration.
    pub(crate) config: Arc<RouterConfig>,
    /// The storage backend handle.
    #[expect(
        dead_code,
        reason = "read by request handlers landing in follow-up issues (#133+)"
    )]
    pub(crate) database: SurrealDatabase,
    #[expect(
        dead_code,
        reason = "read by request handlers landing in follow-up issues (#133+)"
    )]
    /// Router
    pub(crate) router: Arc<SmistaRouter>,
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
/// Construct it with [`WebServer::init`], then call [`WebServer::run`] to bind
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
    pub fn init(config: WebServerConfig) -> anyhow::Result<Self> {
        let host = config.config.host.clone();
        let port = config.config.port;
        let router = Arc::new(SmistaRouter::init(&config.config, config.database.clone())?);
        Ok(Self {
            state: AppState {
                config: Arc::new(config.config),
                database: config.database,
                router,
            },
            host,
            port,
            exit: config.exit,
        })
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
            // The peer address is threaded into each request as `ConnectInfo` so
            // the rate limiter can key buckets per client IP.
            let service = router.into_make_service_with_connect_info::<SocketAddr>();
            if let Err(e) = axum::serve(listener, service)
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

    let mut app = Router::new()
        .route("/status", get(status::status))
        .nest("/api/v1", api)
        // Logging runs outermost so it observes the final status code, including
        // responses short-circuited by the rate limiter and credential guard
        // below.
        .layer(axum::middleware::from_fn(middleware::log_requests));

    // Rate limiting sits inside logging (so rejections are still logged) and
    // outside the credential guard (so the quota is charged before any work).
    // The token bucket cannot be built from a zero period or burst size;
    // validation rejects that before startup, so a `None` config here is only a
    // defensive fallback that leaves the layer off.
    let rate_limit = &state.config.rate_limit;
    if rate_limit.enabled {
        let base = || {
            let mut builder = GovernorConfigBuilder::default();
            builder
                .period(Duration::from_millis(rate_limit.period_ms))
                .burst_size(rate_limit.burst_size);
            builder
        };
        // When `trust_proxy_headers` is set the bucket key comes from the
        // `X-Forwarded-For`/`X-Real-IP`/`Forwarded` headers, which is only safe
        // behind a trusted proxy that overwrites them; otherwise a client could
        // forge a fresh IP per request and slip past the limit. The default keys
        // by the TCP peer address carried in `ConnectInfo`.
        let layered = if rate_limit.trust_proxy_headers {
            base()
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .map(|config| app.clone().layer(GovernorLayer::new(config)))
        } else {
            base()
                .finish()
                .map(|config| app.clone().layer(GovernorLayer::new(config)))
        };
        match layered {
            Some(layered) => app = layered,
            None => tracing::error!(
                "rate limiting is enabled but the configuration is degenerate; rate limiting disabled"
            ),
        }
    }

    // The credential guard runs before the extractor so a credential smuggled
    // through the query string is rejected outright; the extractor then lifts
    // the header credentials into the request extensions for the handler.
    app.layer(axum::middleware::from_fn(
        middleware::reject_query_credentials,
    ))
    .layer(axum::middleware::from_fn(
        middleware::extract_provider_credentials,
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
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    use axum::Router;
    use axum::body::Body;
    use axum::extract::ConnectInfo;
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

    pub(crate) fn test_smista_router() -> Arc<super::SmistaRouter> {
        Arc::new(super::SmistaRouter::mock())
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
            router: test_smista_router(),
        };
        build_router(state)
    }

    /// Sends a request through the router and returns the status code together
    /// with the JSON body (or [`serde_json::Value::Null`] when the body is
    /// empty).
    pub(crate) async fn send(
        router: Router,
        mut request: Request<Body>,
    ) -> (StatusCode, serde_json::Value) {
        // The rate limiter keys buckets by peer IP, which it reads from the
        // `ConnectInfo` extension that the real server installs. `oneshot`
        // bypasses that, so default a loopback peer when a test has not set one.
        if request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .is_none()
        {
            request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                0,
            )));
        }
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

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;

    use axum::Router;
    use axum::extract::ConnectInfo;
    use axum::http::StatusCode;
    use tower::ServiceExt as _;

    use super::test_support::{get, test_database, test_smista_router};
    use super::{AppState, build_router};
    use crate::config::{RateLimitConfig, RouterConfig};

    /// Builds a router whose only non-default setting is its rate-limit config.
    async fn router_with_rate_limit(rate_limit: RateLimitConfig) -> Router {
        let state = AppState {
            config: Arc::new(RouterConfig {
                rate_limit,
                ..RouterConfig::default()
            }),
            database: test_database().await,
            router: test_smista_router(),
        };
        build_router(state)
    }

    /// Sends `GET /status` from a fixed loopback peer and returns the status
    /// code. Unlike `test_support::send`, it does not parse the body as JSON, so
    /// it tolerates the plain-text body the rate limiter returns on a rejection.
    async fn status_code(router: Router) -> StatusCode {
        status_code_forwarded_for(router, None).await
    }

    /// As [`status_code`], but optionally sets `X-Forwarded-For` so the
    /// proxy-header key extractor can be exercised.
    async fn status_code_forwarded_for(router: Router, forwarded_for: Option<&str>) -> StatusCode {
        let mut request = get("/status");
        request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
        )));
        if let Some(client) = forwarded_for {
            request.headers_mut().insert(
                "x-forwarded-for",
                client.parse().expect("invalid forwarded-for value"),
            );
        }
        router
            .oneshot(request)
            .await
            .expect("router failed to handle the request")
            .status()
    }

    #[tokio::test]
    async fn should_reject_requests_over_the_burst_from_one_peer() {
        // A burst of one and a one-minute refill: the first request passes and
        // the second, from the same peer, is rejected within the test window.
        // Cloning the router shares the same limiter state across requests.
        let router = router_with_rate_limit(RateLimitConfig {
            enabled: true,
            period_ms: 60_000,
            burst_size: 1,
            trust_proxy_headers: false,
        })
        .await;

        assert_eq!(status_code(router.clone()).await, StatusCode::OK);
        assert_eq!(
            status_code(router.clone()).await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn should_not_limit_when_disabled() {
        let router = router_with_rate_limit(RateLimitConfig {
            enabled: false,
            period_ms: 60_000,
            burst_size: 1,
            trust_proxy_headers: false,
        })
        .await;

        for _ in 0..5 {
            assert_eq!(status_code(router.clone()).await, StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn should_key_by_forwarded_header_when_trusting_proxy() {
        // Burst of one, refilling once a minute. With proxy trust on, the bucket
        // key is the `X-Forwarded-For` client, not the (shared) peer address.
        let router = router_with_rate_limit(RateLimitConfig {
            enabled: true,
            period_ms: 60_000,
            burst_size: 1,
            trust_proxy_headers: true,
        })
        .await;

        // Two distinct forwarded clients get independent buckets: both pass.
        assert_eq!(
            status_code_forwarded_for(router.clone(), Some("203.0.113.1")).await,
            StatusCode::OK
        );
        assert_eq!(
            status_code_forwarded_for(router.clone(), Some("203.0.113.2")).await,
            StatusCode::OK
        );
        // A repeat from the first client exhausts its bucket and is rejected.
        assert_eq!(
            status_code_forwarded_for(router.clone(), Some("203.0.113.1")).await,
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}

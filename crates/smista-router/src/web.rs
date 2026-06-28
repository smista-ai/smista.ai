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
//! - One request module per endpoint (for example `routes::status` or
//!   `routes::execute`), each owning a single handler. Endpoints not yet
//!   implemented return a `501 Not Implemented` `error::WebError` until their
//!   owning issue fills them in.
//! - Cross-cutting `middleware`: structured request logging with secrets kept
//!   out of the logs, and rejection of any credential passed as a query
//!   parameter.
//!
//! Domain errors are mapped to the wire format by
//! [`ApiErrorResponse`](smista_core::api::ApiErrorResponse) in `smista-core` and
//! rendered as JSON by `error::WebError`.

pub(crate) mod error;
mod middleware;
#[cfg(feature = "openapi")]
mod openapi;
mod routes;
mod streaming;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use secrecy::SecretString;
use smista_storage::database::surreal::SurrealDatabase;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use uuid::Uuid;

use crate::auth::Authenticator;
use crate::config::RouterConfig;
use crate::router::Router as SmistaRouter;

/// Authenticated user identity extracted from a request by the
/// `middleware::authenticate` middleware.
///
/// Put inside request Extensions so handlers can access it without re-authenticating.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub token: SecretString,
}

/// Shared state threaded into every request handler.
///
/// Cloned once per request by `axum`, so each field is itself cheap to clone:
/// the configuration is shared behind an [`Arc`] and the database handle is a
/// shared connection.
#[derive(Debug, Clone)]
pub(crate) struct AppState {
    /// Authenticator for handling user authentication and session management.
    pub(crate) authenticator: Arc<Authenticator>,
    /// The validated router configuration.
    pub(crate) config: Arc<RouterConfig>,
    /// The storage backend handle.
    pub(crate) database: SurrealDatabase,
    /// The router, holding the providers requests are routed to.
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
        let token_ttl = Duration::from_secs(config.config.auth.token_ttl_seconds);
        let router = Arc::new(SmistaRouter::init(&config.config, config.database.clone())?);
        Ok(Self {
            state: AppState {
                authenticator: Arc::new(Authenticator::new(config.database.clone(), token_ttl)),
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
///
/// Under `/api/v1` the endpoints are split into a public group (bootstrap and
/// sign-in, which cannot require a token of their own) and a protected group
/// gated by the [`authenticate`](middleware::authenticate) middleware. The guard
/// is attached with `route_layer`, so it runs only on the protected routes and
/// never on the public routes or on an unmatched path.
fn build_router(state: AppState) -> Router {
    // Public endpoints accept anonymous callers: bootstrap mints the first key
    // and sign-in exchanges credentials for a session token, so neither can
    // require a token of its own.
    let public = Router::new()
        .route("/auth/bootstrap", post(routes::bootstrap))
        .route("/auth/sign-in", post(routes::sign_in));

    // Protected endpoints are gated by [`authenticate`], applied with
    // `route_layer` so it runs only on these matched routes and never on a 404
    // or on the public routes above. The layer carries its own authenticator
    // state, independent of the application `with_state` below.
    let protected = Router::new()
        .route("/auth/sign-out", post(routes::sign_out))
        .route("/auth/me", get(routes::me))
        .route(
            "/sessions",
            post(routes::create_session).get(routes::list_sessions),
        )
        .route(
            "/sessions/{session_id}",
            get(routes::get_session)
                .put(routes::update_session)
                .delete(routes::delete_session),
        )
        .route("/sessions/{session_id}/execute", post(routes::execute))
        .route("/sessions/{session_id}/preview", post(routes::preview))
        .route(
            "/sessions/{session_id}/continue",
            post(routes::continue_run),
        )
        .route("/sessions/{session_id}/traces", get(routes::get_traces))
        .route("/sessions/{session_id}/usage", get(routes::session_usage))
        .route("/llm/providers", get(routes::list_providers))
        .route("/llm/models", get(routes::list_models))
        .route_layer(axum::middleware::from_fn_with_state(
            state.authenticator.clone(),
            middleware::authenticate,
        ));

    let api = public.merge(protected);

    let mut app = Router::new()
        .route("/status", get(routes::status))
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
    use secrecy::ExposeSecret as _;
    use smista_storage::database::surreal::{SurrealBackend, SurrealDatabase, SurrealOptions};
    use tower::ServiceExt as _;

    use super::{AppState, Authenticator, RouterConfig, build_router};

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
        test_router_with_database().await.0
    }

    /// Like [`test_router`] but also returns the [`SurrealDatabase`] handle the
    /// router writes through, so a test can assert what an endpoint persisted.
    pub(crate) async fn test_router_with_database() -> (Router, SurrealDatabase) {
        let database = test_database().await;
        let config = RouterConfig::default();
        let token_ttl = std::time::Duration::from_secs(config.auth.token_ttl_seconds);
        let state = AppState {
            authenticator: Arc::new(super::Authenticator::new(database.clone(), token_ttl)),
            config: Arc::new(config),
            database: database.clone(),
            router: test_smista_router(),
        };
        (build_router(state), database)
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

    /// Sends a request and returns the status, `Content-Type` and the raw body,
    /// so a streaming response can be asserted without parsing it as one JSON value.
    pub(crate) async fn send_raw(
        router: Router,
        mut request: Request<Body>,
    ) -> (StatusCode, String, String) {
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
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
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
    pub(crate) fn sse_events(body: &str) -> Vec<serde_json::Value> {
        body.split("\n\n")
            .filter_map(|record| record.strip_prefix("data: "))
            .map(|json| serde_json::from_str(json).expect("event payload was not valid JSON"))
            .collect()
    }

    /// Sends a request through the router and returns only the status code.
    ///
    /// Unlike [`send`], it does not parse the body as JSON, so it tolerates the
    /// plain-text body axum returns when a request fails to deserialize.
    pub(crate) async fn send_status(router: Router, mut request: Request<Body>) -> StatusCode {
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
        router
            .oneshot(request)
            .await
            .expect("router failed to handle the request")
            .status()
    }

    /// Builds an empty `GET` request for `uri`.
    pub(crate) fn get(uri: &str) -> Request<Body> {
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds an empty `POST` request for `uri`.
    pub(crate) fn post(uri: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds an empty `POST` request for `uri` carrying `api_key` in the
    /// `X-Smista-Api-Key` header, as the sign-in endpoint expects.
    pub(crate) fn post_with_api_key(uri: &str, api_key: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("X-Smista-Api-Key", api_key)
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds an empty `GET` request for `uri` carrying `token` as a Bearer
    /// credential in the `Authorization` header.
    pub(crate) fn get_with_token(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds an empty `POST` request for `uri` carrying `token` as a Bearer
    /// credential in the `Authorization` header.
    pub(crate) fn post_with_token(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds an empty `DELETE` request for `uri`.
    pub(crate) fn delete(uri: &str) -> Request<Body> {
        Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds an empty `DELETE` request for `uri` carrying `token` as a Bearer
    /// credential in the `Authorization` header.
    pub(crate) fn delete_with_token(uri: &str, token: &str) -> Request<Body> {
        Request::builder()
            .method(Method::DELETE)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds a `POST` request for `uri` carrying `token` as a Bearer credential
    /// and `body` serialized as a JSON request body.
    pub(crate) fn post_json_with_token(
        uri: &str,
        token: &str,
        body: &serde_json::Value,
    ) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(body).expect("failed to serialize request body"),
            ))
            .expect("failed to build request")
    }

    /// Builds an empty `PUT` request for `uri`.
    pub(crate) fn put(uri: &str) -> Request<Body> {
        Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request")
    }

    /// Builds a `PUT` request for `uri` carrying `token` as a Bearer credential
    /// and `body` serialized as a JSON request body.
    pub(crate) fn put_json_with_token(
        uri: &str,
        token: &str,
        body: &serde_json::Value,
    ) -> Request<Body> {
        Request::builder()
            .method(Method::PUT)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::to_vec(body).expect("failed to serialize request body"),
            ))
            .expect("failed to build request")
    }

    /// Builds the application router alongside a valid Bearer token for a freshly
    /// bootstrapped user.
    ///
    /// Mirrors [`test_router`] but also bootstraps a user and signs them in over
    /// the same authenticator the router uses, so tests can drive the
    /// authenticated routes end to end. Returns the router and the raw token
    /// string to place in the `Authorization` header.
    pub(crate) async fn authenticated_router() -> (Router, String) {
        let (router, token, _user_id) = authenticated_router_with_user().await;
        (router, token)
    }

    /// Like [`authenticated_router`] but also returns the bootstrapped user's
    /// [`Uuid`](uuid::Uuid), so a test can assert which user a token resolves to.
    pub(crate) async fn authenticated_router_with_user() -> (Router, String, uuid::Uuid) {
        let (router, token, user_id, _db) = authenticated_router_with_database().await;
        (router, token, user_id)
    }

    /// Like [`authenticated_router_with_user`] but also returns the
    /// [`SurrealDatabase`] handle the router writes through, so a test can assert
    /// what an authenticated endpoint persisted.
    pub(crate) async fn authenticated_router_with_database()
    -> (Router, String, uuid::Uuid, SurrealDatabase) {
        authenticated_router_with_router(test_smista_router()).await
    }

    /// Like [`authenticated_router_with_database`] but routes through the given
    /// [`SmistaRouter`](super::SmistaRouter), so a test can install a scripted
    /// mock and drive the execution endpoints through a specific outcome (a tool
    /// pause, a plan approval) rather than the default completing mock.
    pub(crate) async fn authenticated_router_with_router(
        router: Arc<super::SmistaRouter>,
    ) -> (Router, String, uuid::Uuid, SurrealDatabase) {
        let database = test_database().await;
        let config = RouterConfig::default();
        let token_ttl = std::time::Duration::from_secs(config.auth.token_ttl_seconds);
        let authenticator = Arc::new(Authenticator::new(database.clone(), token_ttl));
        let user = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap the test user");
        let session = authenticator
            .sign_in(&user.api_key)
            .await
            .expect("failed to sign the test user in");
        let token = session.token.expose_secret().to_string();
        let state = AppState {
            authenticator,
            config: Arc::new(config),
            database: database.clone(),
            router,
        };
        (build_router(state), token, user.user_id, database)
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

    use super::test_support::{
        authenticated_router, get, get_with_token, post, send, test_database, test_smista_router,
    };
    use super::{AppState, build_router};
    use crate::config::{RateLimitConfig, RouterConfig};

    /// Builds a router whose only non-default setting is its rate-limit config.
    async fn router_with_rate_limit(rate_limit: RateLimitConfig) -> Router {
        let database = test_database().await;
        let token_ttl =
            std::time::Duration::from_secs(RouterConfig::default().auth.token_ttl_seconds);
        let state = AppState {
            authenticator: Arc::new(super::Authenticator::new(database.clone(), token_ttl)),
            config: Arc::new(RouterConfig {
                rate_limit,
                ..RouterConfig::default()
            }),
            database,
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

    #[tokio::test]
    async fn should_allow_public_routes_without_a_token() {
        // Bootstrap is public: with no `Authorization` header the request must
        // reach the handler and succeed rather than being rejected by the auth
        // guard, proving the guard does not wrap the public routes.
        let (router, _token) = authenticated_router().await;
        let (status, body) = send(router, post("/api/v1/auth/bootstrap")).await;

        assert_eq!(status, StatusCode::CREATED);
        assert!(body["api_key"].is_string());
    }

    #[tokio::test]
    async fn should_reject_protected_routes_without_a_token() {
        let (router, _token) = authenticated_router().await;
        let (status, body) = send(router, get("/api/v1/auth/me")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_allow_protected_routes_with_a_valid_token() {
        // A valid token clears the guard, so the request reaches the handler and
        // gets its real response rather than a 401.
        let (router, token) = authenticated_router().await;
        let (status, body) = send(router, get_with_token("/api/v1/auth/me", &token)).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["user_id"].is_string());
    }

    #[tokio::test]
    async fn should_not_authenticate_unknown_routes() {
        // The guard is attached with `route_layer`, so an unmatched path falls
        // through to a 404 instead of being challenged for a token first.
        let (router, _token) = authenticated_router().await;
        let (status, _body) = send(router, get("/api/v1/does-not-exist")).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

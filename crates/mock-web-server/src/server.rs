use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Uri};
use axum::response::Response;
use axum::routing::{get, post};
use smista_sdk::core::api::ApiErrorCode;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::endpoint::{Endpoint, EndpointStatus};
use crate::request::Request;
use crate::response::{ResponseTemplate, api_error};

/// A running mock of the `smista-router` HTTP API.
pub struct MockRouter {
    /// The server's base URL.
    uri: String,
    /// Shared server state.
    state: Arc<MockRouterState>,
    /// Token used to stop the server.
    cancellation: CancellationToken,
    /// Background server thread.
    handle: Option<JoinHandle<()>>,
    /// Permit limiting concurrent mock servers in parallel test runs.
    _permit: OwnedSemaphorePermit,
}

impl MockRouter {
    /// Starts a mock router serving every endpoint's default response.
    pub async fn start() -> Self {
        Self::run(CancellationToken::new()).await
    }

    /// Starts a mock router that stops when `cancellation` is cancelled.
    pub async fn run(cancellation: CancellationToken) -> Self {
        Self::builder().run(cancellation).await
    }

    /// Begins building a mock router whose responses can be overridden.
    pub fn builder() -> MockRouterBuilder {
        MockRouterBuilder::default()
    }

    /// The server's base URL, shaped for router client configuration.
    #[must_use]
    pub fn base_url(&self) -> Url {
        Url::parse(&self.uri).expect("the mock router URI is a valid base URL")
    }

    /// The server's base URL as a string.
    #[must_use]
    pub fn uri(&self) -> String {
        self.uri.clone()
    }

    /// Returns the requests received so far, in arrival order.
    pub async fn received_requests(&self) -> Vec<Request> {
        self.state.received.lock().await.clone()
    }

    /// Waits for the server task to stop.
    pub async fn wait_stopped(mut self) {
        if let Some(handle) = self.handle.take() {
            tokio::task::spawn_blocking(move || {
                handle
                    .join()
                    .expect("the mock router thread should not panic");
            })
            .await
            .expect("the mock router join task should not panic");
        }
    }
}

impl Drop for MockRouter {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .expect("the mock router thread should not panic");
        }
    }
}

/// Builder for a [`MockRouter`] with per-endpoint overrides.
#[derive(Default)]
pub struct MockRouterBuilder {
    /// Response status overrides.
    statuses: HashMap<Endpoint, EndpointStatus>,
    /// Success body overrides.
    responses: HashMap<Endpoint, ResponseTemplate>,
}

impl MockRouterBuilder {
    /// Configures the status served for `endpoint`.
    ///
    /// # Panics
    ///
    /// Panics when [`EndpointStatus::NotFound`] is configured for an endpoint
    /// that has no documented 404 resource case.
    #[must_use]
    pub fn endpoint_status(mut self, endpoint: Endpoint, status: EndpointStatus) -> Self {
        if status == EndpointStatus::NotFound && !endpoint.allows_not_found() {
            panic!("Endpoint {endpoint:?} does not support NotFound");
        }
        self.statuses.insert(endpoint, status);
        self
    }

    /// Replaces the successful response served for `endpoint`.
    #[must_use]
    pub fn respond(mut self, endpoint: Endpoint, response: ResponseTemplate) -> Self {
        self.responses.insert(endpoint, response);
        self
    }

    /// Starts the server, stopping it when `cancellation` is cancelled.
    pub async fn run(self, cancellation: CancellationToken) -> MockRouter {
        let permit = mock_server_slots()
            .acquire_owned()
            .await
            .expect("the mock router semaphore should not be closed");
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .expect("the mock router should bind an ephemeral port");
        let address = listener
            .local_addr()
            .expect("the mock router listener exposes its local address");
        listener
            .set_nonblocking(true)
            .expect("the mock router listener can be made nonblocking");
        let uri = uri_from_address(address);
        let state = Arc::new(MockRouterState {
            base_url: Url::parse(&uri).expect("the mock router URI is a valid base URL"),
            statuses: self.statuses,
            responses: self.responses,
            received: Mutex::new(Vec::new()),
        });
        let app = router(Arc::clone(&state));
        let shutdown = cancellation.clone();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("the mock router runtime should build");
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)
                    .expect("the mock router listener can enter Tokio");
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        shutdown.cancelled().await;
                    })
                    .await
                    .expect("the mock router server should stop cleanly");
            });
        });

        MockRouter {
            uri,
            state,
            cancellation,
            handle: Some(handle),
            _permit: permit,
        }
    }

    /// Starts the server with a fresh internal cancellation token.
    pub async fn start(self) -> MockRouter {
        self.run(CancellationToken::new()).await
    }
}

/// Shared server state.
struct MockRouterState {
    /// Base URL used to build recorded request URLs.
    base_url: Url,
    /// Response status overrides.
    statuses: HashMap<Endpoint, EndpointStatus>,
    /// Success body overrides.
    responses: HashMap<Endpoint, ResponseTemplate>,
    /// Recorded requests.
    received: Mutex<Vec<Request>>,
}

impl MockRouterState {
    /// Returns the configured status for an endpoint.
    fn endpoint_status(&self, endpoint: Endpoint) -> EndpointStatus {
        self.statuses
            .get(&endpoint)
            .copied()
            .unwrap_or(EndpointStatus::Ok)
    }

    /// Returns the configured success response for an endpoint.
    fn response(&self, endpoint: Endpoint) -> ResponseTemplate {
        self.responses
            .get(&endpoint)
            .cloned()
            .unwrap_or_else(|| endpoint.default_response())
    }
}

/// Builds the HTTP router.
fn router(state: Arc<MockRouterState>) -> Router {
    Router::new()
        .route("/status", get(handle_status))
        .route("/api/v1/auth/bootstrap", post(handle_bootstrap))
        .route("/api/v1/auth/sign-in", post(handle_sign_in))
        .route("/api/v1/auth/sign-out", post(handle_sign_out))
        .route("/api/v1/auth/me", get(handle_me))
        .route(
            "/api/v1/sessions",
            post(handle_create_session).get(handle_list_sessions),
        )
        .route(
            "/api/v1/sessions/{session_id}",
            get(handle_get_session)
                .put(handle_update_session)
                .delete(handle_delete_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/execute",
            post(handle_execute),
        )
        .route(
            "/api/v1/sessions/{session_id}/continue",
            post(handle_continue_run),
        )
        .route(
            "/api/v1/sessions/{session_id}/preview",
            post(handle_preview),
        )
        .route(
            "/api/v1/sessions/{session_id}/traces",
            get(handle_get_traces),
        )
        .route("/api/v1/llm/providers", get(handle_list_providers))
        .route("/api/v1/llm/models", get(handle_list_models))
        .route(
            "/api/v1/sessions/{session_id}/usage",
            get(handle_session_usage),
        )
        .with_state(state)
}

macro_rules! endpoint_handler {
    ($name:ident, $endpoint:expr) => {
        async fn $name(
            State(state): State<Arc<MockRouterState>>,
            method: Method,
            uri: Uri,
            headers: HeaderMap,
            body: Bytes,
        ) -> Response {
            handle($endpoint, state, method, uri, headers, body).await
        }
    };
}

endpoint_handler!(handle_status, Endpoint::Status);
endpoint_handler!(handle_bootstrap, Endpoint::Bootstrap);
endpoint_handler!(handle_sign_in, Endpoint::SignIn);
endpoint_handler!(handle_sign_out, Endpoint::SignOut);
endpoint_handler!(handle_me, Endpoint::Me);
endpoint_handler!(handle_create_session, Endpoint::CreateSession);
endpoint_handler!(handle_list_sessions, Endpoint::ListSessions);
endpoint_handler!(handle_get_session, Endpoint::GetSession);
endpoint_handler!(handle_update_session, Endpoint::UpdateSession);
endpoint_handler!(handle_delete_session, Endpoint::DeleteSession);
endpoint_handler!(handle_execute, Endpoint::Execute);
endpoint_handler!(handle_continue_run, Endpoint::ContinueRun);
endpoint_handler!(handle_preview, Endpoint::Preview);
endpoint_handler!(handle_get_traces, Endpoint::GetTraces);
endpoint_handler!(handle_list_providers, Endpoint::ListProviders);
endpoint_handler!(handle_list_models, Endpoint::ListModels);
endpoint_handler!(handle_session_usage, Endpoint::SessionUsage);

/// Records a request and returns the configured endpoint response.
async fn handle(
    endpoint: Endpoint,
    state: Arc<MockRouterState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    state.received.lock().await.push(received_request(
        &state.base_url,
        method,
        uri,
        headers,
        body,
    ));

    match state.endpoint_status(endpoint) {
        EndpointStatus::Ok => state.response(endpoint).into_response(),
        EndpointStatus::Unauthorized => {
            api_error(ApiErrorCode::MissingCredentials, "Unauthorized").into_response()
        }
        EndpointStatus::ServerError => {
            api_error(ApiErrorCode::InternalError, "Server error").into_response()
        }
        EndpointStatus::NotFound => {
            api_error(ApiErrorCode::SessionNotFound, "Not found").into_response()
        }
    }
}

/// Converts incoming request pieces into a recorded request.
fn received_request(
    base_url: &Url,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Request {
    let mut url = base_url.clone();
    url.set_path(uri.path());
    url.set_query(uri.query());
    Request {
        url,
        method,
        headers,
        body: body.to_vec(),
    }
}

/// Formats the base URI for a listener address.
fn uri_from_address(address: SocketAddr) -> String {
    format!("http://{address}")
}

/// Returns the process-wide concurrency gate for live mock servers.
fn mock_server_slots() -> Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    Arc::clone(SLOTS.get_or_init(|| Arc::new(Semaphore::new(4))))
}

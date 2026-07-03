//! An [`isahc`]-backed [`Client`], a runtime-agnostic async HTTP backend.
//!
//! [`IsahcClient`] is a concrete [`Client`](crate::Client) implementation built on
//! [`isahc`], a libcurl-based HTTP client that drives its transfers on its own
//! background agent thread. It is gated behind the crate's `isahc` feature and
//! exists for frontends that want a fully `async` client without committing to a
//! `tokio` reactor: every future it returns is `Send` and resolves on **any**
//! executor — including a trivial one such as [`futures::executor::block_on`] —
//! because the I/O happens on `isahc`'s own thread, not the polling runtime.
//!
//! # No runtime of the caller's
//!
//! Unlike [`ReqwestClient`], which needs a `tokio` reactor, this backend carries
//! no async runtime requirement: `isahc` owns the event loop. The [`Client`](crate::Client)
//! methods are genuine `async fn`s whose futures register with `isahc`'s agent
//! rather than the caller's executor, so the same client works under Tokio,
//! `async-std`, `smol`, or a bare `block_on`.
//!
//! # Held state
//!
//! The client owns its credentials so the [`Client`](crate::Client) methods take
//! `&self` and no per-call credential. [`bootstrap`](Client::bootstrap) stores the
//! API key it mints, [`sign_in`](Client::sign_in) exchanges that key for a session
//! token it then holds, and [`sign_out`](Client::sign_out) clears the token. The
//! API key and token live behind `Arc<RwLock<…>>`, so a [`Clone`] of the client
//! shares the same auth state, and an authenticated call made before a token is
//! held fails with [`NotAuthenticated`](RouterClientError::NotAuthenticated)
//! without a network round trip.
//!
//! The provider keys the router needs to reach a model
//! ([`ProviderCredentials`]) are configured on the client too and travel as
//! request headers on the methods that can call a model.
//!
//! # Wire behaviour
//!
//! Versioned endpoints are prefixed with `/api/v1`; the health check is `/status`
//! at the root. A non-success response is decoded as the router's structured
//! [`ApiError`] body and paired with the HTTP status; transport and decode
//! failures map to [`RouterClientError`]. The streaming methods negotiate
//! `text/event-stream` and parse the Server-Sent Events frames into
//! [`TurnEvent`]s, ending after the terminal `turn_end`.
//!
//! Credentials travel only in headers — never in a query parameter, log, trace or
//! error — and the TLS backend is `rustls`, so a static musl build needs no C
//! toolchain.
//!
//! [`ReqwestClient`]: crate::ReqwestClient

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::{Arc, RwLock};

use futures::stream::{self, BoxStream};
use futures::{AsyncReadExt as _, StreamExt as _};
use isahc::config::Configurable as _;
use isahc::http::header::{AUTHORIZATION, CONTENT_TYPE};
use isahc::http::request::Builder;
use isahc::{AsyncBody, AsyncReadResponseExt as _, HttpClient, Request, Response};
use smista_core::api::{
    ApiError, BootstrapResponse, ContinueRequest, CreateSessionRequest, CreateSessionResponse,
    DeleteSessionResponse, ExecuteRequest, GetSessionResponse, ListModelsResponse,
    ListProvidersResponse, ListSessionsResponse, MeResponse, PreviewResponse, SessionUsageResponse,
    SignInResponse, SignOutResponse, StatusResponse, TraceResponse, TurnEvent, TurnResponse,
    UpdateSessionRequest, UpdateSessionResponse,
};
use smista_core::credential::{ApiKey, SessionToken};
use url::Url;
use uuid::Uuid;

use crate::error::Result;
use crate::{Client, ProviderCredentials, RouterClientConfig, RouterClientError};

/// Bytes read from the streaming socket per poll, large enough to hold a typical
/// SSE record in one read yet small enough to keep the buffer footprint modest.
const STREAM_READ_CHUNK: usize = 8 * 1024;

/// An [`isahc`]-backed, runtime-agnostic async [`Client`](crate::Client)
/// implementation.
///
/// Holds the router base URL and timeouts, an API key and session token behind
/// shared interior mutability, and the provider credentials forwarded to the
/// router. It owns its credentials, so the [`Client`](crate::Client) methods take
/// `&self` with no per-call credential; a [`Clone`] shares the same auth state;
/// and an authenticated call made before a token is held fails with
/// [`NotAuthenticated`](RouterClientError::NotAuthenticated) without a network
/// round trip.
///
/// Every future is `Send` and driven by `isahc`'s own agent thread, so the client
/// runs under any executor without a `tokio` reactor of its own.
///
/// # Examples
///
/// ```no_run
/// use smista_router_client::{Client, IsahcClient, RouterClientConfig};
///
/// # async fn run() -> smista_router_client::Result<()> {
/// let client = IsahcClient::new(RouterClientConfig::default())?;
/// // Bootstrap mints and stores the API key, sign-in stores the session token.
/// client.bootstrap().await?;
/// client.sign_in().await?;
/// let sessions = client.list_sessions(None, None).await?;
/// # let _ = sessions;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct IsahcClient {
    /// The held router API key, populated by [`bootstrap`](Client::bootstrap) or
    /// seeded with [`with_api_key`](Self::with_api_key); `None` until then.
    api_key: Arc<RwLock<Option<ApiKey>>>,
    /// The shared, cheaply cloned HTTP backend carrying the configured timeouts.
    client: HttpClient,
    /// Connection settings: base URL and timeouts.
    config: RouterClientConfig,
    /// Provider keys forwarded to the router on the model-calling methods.
    credentials: Arc<ProviderCredentials>,
    /// The held session token, populated by [`sign_in`](Client::sign_in) and
    /// cleared by [`sign_out`](Client::sign_out); `None` until then.
    session_token: Arc<RwLock<Option<SessionToken>>>,
}

impl TryFrom<RouterClientConfig> for IsahcClient {
    type Error = RouterClientError;

    /// Builds a client for `config` with no credentials held.
    ///
    /// # Errors
    ///
    /// Returns [`RouterClientError::ClientBuild`] if the underlying HTTP backend
    /// cannot be constructed (for example a TLS backend initialization failure).
    fn try_from(config: RouterClientConfig) -> Result<Self> {
        let client = HttpClient::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.request_timeout)
            .build()
            .map_err(|error| RouterClientError::ClientBuild(error.to_string()))?;

        Ok(Self {
            api_key: Arc::new(RwLock::new(None)),
            client,
            config,
            credentials: Arc::new(ProviderCredentials::default()),
            session_token: Arc::new(RwLock::new(None)),
        })
    }
}

impl IsahcClient {
    /// Creates a client for `config` holding no API key or session token.
    ///
    /// # Errors
    ///
    /// Returns [`RouterClientError::ClientBuild`] if the HTTP backend cannot be
    /// constructed.
    pub fn new(config: RouterClientConfig) -> Result<Self> {
        Self::try_from(config)
    }

    /// Seeds the held API key, returning the updated client for chaining.
    ///
    /// Use this when the API key is already known (for example loaded from a
    /// keyring), so [`sign_in`](Client::sign_in) can run without a prior
    /// [`bootstrap`](Client::bootstrap).
    #[must_use]
    pub fn with_api_key(self, api_key: ApiKey) -> Self {
        *self.api_key.write().expect("the API key lock is poisoned") = Some(api_key);
        self
    }

    /// Sets the provider credentials forwarded to the router, for chaining.
    #[must_use]
    pub fn with_provider_credentials(mut self, credentials: ProviderCredentials) -> Self {
        self.credentials = Arc::new(credentials);
        self
    }

    /// Seeds the held session token, returning the updated client for chaining.
    ///
    /// Use this to restore a previously obtained, still-valid token without
    /// signing in again.
    #[must_use]
    pub fn with_session_token(self, session_token: SessionToken) -> Self {
        *self
            .session_token
            .write()
            .expect("the session token lock is poisoned") = Some(session_token);
        self
    }

    /// Joins `path` onto the configured base URL.
    fn url(&self, path: &str) -> Result<Url> {
        Ok(self.config.base_url.join(path)?)
    }

    /// Returns a clone of the held session token, or
    /// [`NotAuthenticated`](RouterClientError::NotAuthenticated) if none is held.
    ///
    /// This is the no-round-trip guard every authenticated method calls first.
    fn session_token(&self) -> Result<SessionToken> {
        match self
            .session_token
            .read()
            .expect("the session token lock is poisoned")
            .clone()
        {
            Some(token) => Ok(token),
            None => {
                tracing::warn!("authenticated request attempted without a session token");
                Err(RouterClientError::NotAuthenticated)
            }
        }
    }

    /// Adds the `Authorization: Bearer` header from the held session token.
    ///
    /// Fails with [`NotAuthenticated`](RouterClientError::NotAuthenticated) before
    /// a token is held, so authenticated calls short-circuit without a request.
    fn authorize(&self, builder: Builder) -> Result<Builder> {
        let token = self.session_token()?;
        Ok(builder.header(AUTHORIZATION, format!("Bearer {}", token.expose())))
    }

    /// Adds an `X-Smista-Provider-<provider>-Api-Key` header per held provider key.
    fn provider_headers(&self, builder: Builder) -> Builder {
        self.credentials
            .headers_map()
            .into_iter()
            .fold(builder, |builder, (name, value)| {
                builder.header(name, value)
            })
    }

    /// Sends `request` and decodes a success body or maps an error response.
    async fn send<T, B>(&self, request: Request<B>) -> Result<T>
    where
        T: serde::de::DeserializeOwned + Unpin,
        B: Into<AsyncBody>,
    {
        let response = self
            .client
            .send_async(request)
            .await
            .map_err(transport_error)?;
        decode(response).await
    }

    /// Sends `request` and opens its body as an SSE stream of [`TurnEvent`]s.
    ///
    /// A non-success status is mapped to an [`ApiError`] before the stream opens;
    /// once open, each event is parsed lazily and a mid-stream failure surfaces as
    /// an [`Err`] item.
    async fn open_stream<B>(
        &self,
        request: Request<B>,
    ) -> Result<BoxStream<'static, Result<TurnEvent>>>
    where
        B: Into<AsyncBody>,
    {
        let response = self
            .client
            .send_async(request)
            .await
            .map_err(transport_error)?;

        if response.status().is_success() {
            Ok(sse_events(response))
        } else {
            Err(decode_api_error(response).await)
        }
    }
}

/// Builds an empty-bodied request from `builder`.
fn empty_request(builder: Builder) -> Result<Request<()>> {
    builder.body(()).map_err(request_build_error)
}

/// Builds a JSON-bodied request from `builder`, serializing `body`.
fn json_request<B>(builder: Builder, body: &B) -> Result<Request<Vec<u8>>>
where
    B: serde::Serialize,
{
    let bytes = serde_json::to_vec(body)
        .map_err(|error| RouterClientError::RequestBuild(error.to_string()))?;
    builder
        .header(CONTENT_TYPE, "application/json")
        .body(bytes)
        .map_err(request_build_error)
}

/// Logs and wraps an [`isahc`] transport failure as a [`RouterClientError`].
///
/// The error never carries a credential: credentials live in request headers, not
/// the URL, so the logged [`Display`](std::fmt::Display) form is safe.
fn transport_error(error: isahc::Error) -> RouterClientError {
    tracing::debug!(error = %error, "the HTTP backend failed to complete a request");
    RouterClientError::transport(error)
}

/// Logs and wraps a mid-stream read failure as a [`RouterClientError`].
fn read_error(error: std::io::Error) -> RouterClientError {
    tracing::debug!(error = %error, "the HTTP backend failed while reading a stream");
    RouterClientError::transport(error)
}

/// Wraps a request-builder failure (such as an invalid header) as a
/// [`RouterClientError`].
fn request_build_error(error: isahc::http::Error) -> RouterClientError {
    tracing::debug!(error = %error, "failed to build the HTTP request");
    RouterClientError::RequestBuild(error.to_string())
}

/// Decodes a success body, or the router's [`ApiError`] on a non-2xx status.
async fn decode<T>(mut response: Response<AsyncBody>) -> Result<T>
where
    T: serde::de::DeserializeOwned + Unpin,
{
    if response.status().is_success() {
        response
            .json::<T>()
            .await
            .map_err(|error| RouterClientError::Decode(error.to_string()))
    } else {
        Err(decode_api_error(response).await)
    }
}

/// Decodes a non-success response into a [`RouterClientError`].
///
/// Returns an [`Api`](RouterClientError::Api) error when the body is the router's
/// structured [`ApiError`], or a [`Decode`](RouterClientError::Decode) error when
/// the body cannot be parsed.
async fn decode_api_error(mut response: Response<AsyncBody>) -> RouterClientError {
    let status = response.status().as_u16();
    match response.json::<ApiError>().await {
        Ok(body) => {
            tracing::debug!(
                status,
                code = %body.error.code,
                "router returned an API error response"
            );
            RouterClientError::api(status, body)
        }
        Err(error) => RouterClientError::Decode(error.to_string()),
    }
}

/// Holds the in-flight state of an SSE [`TurnEvent`] stream.
struct SseState {
    /// The async reader over the response body.
    body: AsyncBody,
    /// Bytes received but not yet split into complete records.
    buffer: Vec<u8>,
    /// Events parsed from drained records, awaiting delivery in order.
    queue: VecDeque<Result<TurnEvent>>,
    /// Whether the stream has reached its terminal event or a fatal failure.
    finished: bool,
}

/// Parses an open SSE response body into a stream of [`TurnEvent`]s.
///
/// Records are split on the blank-line (`\n\n`) separator the router emits, each
/// carrying one JSON `TurnEvent` on a `data:` line. The stream yields each event
/// in order and ends after the terminal `turn_end`; a decode or transport failure
/// surfaces as an [`Err`] item and ends the stream. Each poll reads from the
/// socket through `isahc`'s agent thread, so the polling executor is never blocked.
fn sse_events(response: Response<AsyncBody>) -> BoxStream<'static, Result<TurnEvent>> {
    let state = SseState {
        body: response.into_body(),
        buffer: Vec::new(),
        queue: VecDeque::new(),
        finished: false,
    };

    stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.queue.pop_front() {
                if matches!(event, Ok(TurnEvent::TurnEnd(_))) {
                    // The terminal event closes the turn: deliver it, then stop.
                    state.finished = true;
                }
                return Some((event, state));
            }
            if state.finished {
                return None;
            }
            let mut chunk = [0u8; STREAM_READ_CHUNK];
            match state.body.read(&mut chunk).await {
                Ok(0) => {
                    // The connection closed: flush any final unterminated record.
                    let trailing = std::mem::take(&mut state.buffer);
                    parse_sse_record(&trailing, &mut state.queue);
                    state.finished = true;
                }
                Ok(read) => {
                    state.buffer.extend_from_slice(&chunk[..read]);
                    drain_sse_records(&mut state.buffer, &mut state.queue);
                }
                Err(error) => {
                    // A transport failure ends the turn after it is surfaced.
                    state.queue.push_back(Err(read_error(error)));
                    state.finished = true;
                }
            }
        }
    })
    .boxed()
}

/// Drains every complete `\n\n`-terminated record from `buffer` into `queue`.
fn drain_sse_records(buffer: &mut Vec<u8>, queue: &mut VecDeque<Result<TurnEvent>>) {
    // `\n` is a single byte and never part of a multi-byte UTF-8 sequence, so
    // splitting on the blank-line separator is safe even when a record straddles
    // two reads.
    while let Some(position) = buffer.windows(2).position(|window| window == b"\n\n") {
        let record: Vec<u8> = buffer.drain(..position + 2).collect();
        parse_sse_record(&record[..position], queue);
    }
}

/// Parses one SSE record's `data:` payload into a [`TurnEvent`], queuing it.
fn parse_sse_record(record: &[u8], queue: &mut VecDeque<Result<TurnEvent>>) {
    let text = match std::str::from_utf8(record) {
        Ok(text) => text,
        Err(error) => {
            queue.push_back(Err(RouterClientError::Decode(error.to_string())));
            return;
        }
    };

    // Per the SSE spec, multiple `data:` lines in a record are joined with a
    // newline; comments (`:` lines) and other fields are ignored. The router
    // sends exactly one `data:` line per record.
    let mut data = String::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("data:") {
            let value = value.strip_prefix(' ').unwrap_or(value);
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(value);
        }
    }

    if data.is_empty() {
        // A blank keep-alive or comment-only record carries no event.
        return;
    }

    match serde_json::from_str::<TurnEvent>(&data) {
        Ok(event) => queue.push_back(Ok(event)),
        Err(error) => queue.push_back(Err(RouterClientError::Decode(error.to_string()))),
    }
}

impl Client for IsahcClient {
    fn base_url(&self) -> &Url {
        &self.config.base_url
    }

    async fn status(&self) -> Result<StatusResponse> {
        let url = self.url("/status")?;
        self.send(empty_request(Request::get(url.as_str()))?).await
    }

    async fn bootstrap(&self) -> Result<BootstrapResponse> {
        let url = self.url("/api/v1/auth/bootstrap")?;
        let response: BootstrapResponse = self
            .send(empty_request(Request::post(url.as_str()))?)
            .await?;
        // Bootstrap mints the long-lived API key exactly once: hold it so a later
        // `sign_in` can present it.
        *self.api_key.write().expect("the API key lock is poisoned") = Some(
            ApiKey::from_str(&response.api_key).map_err(|_| RouterClientError::InvalidApiKey)?,
        );
        Ok(response)
    }

    async fn sign_in(&self) -> Result<SignInResponse> {
        let url = self.url("/api/v1/auth/sign-in")?;
        let mut builder = Request::post(url.as_str());
        // The API key is presented only at sign-in, only when one is held; absent
        // it, the router answers with a `missing_credentials` API error.
        if let Some(api_key) = self
            .api_key
            .read()
            .expect("the API key lock is poisoned")
            .as_ref()
        {
            builder = builder.header(ApiKey::header_name(), api_key.expose());
        }
        let response: SignInResponse = self.send(empty_request(builder)?).await?;
        // Hold the minted session token for subsequent authenticated calls.
        *self
            .session_token
            .write()
            .expect("the session token lock is poisoned") = Some(
            SessionToken::from_str(&response.token)
                .map_err(|_| RouterClientError::InvalidSessionToken)?,
        );
        Ok(response)
    }

    async fn sign_out(&self) -> Result<SignOutResponse> {
        let url = self.url("/api/v1/auth/sign-out")?;
        let builder = self.authorize(Request::post(url.as_str()))?;
        let response: SignOutResponse = self.send(empty_request(builder)?).await?;
        // The token is revoked server-side; drop the held copy too.
        *self
            .session_token
            .write()
            .expect("the session token lock is poisoned") = None;
        Ok(response)
    }

    async fn me(&self) -> Result<MeResponse> {
        let url = self.url("/api/v1/auth/me")?;
        let builder = self.authorize(Request::get(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }

    async fn create_session(&self, req: CreateSessionRequest) -> Result<CreateSessionResponse> {
        let url = self.url("/api/v1/sessions")?;
        let builder = self.authorize(Request::post(url.as_str()))?;
        self.send(json_request(builder, &req)?).await
    }

    async fn list_sessions(
        &self,
        scope: Option<String>,
        title: Option<String>,
    ) -> Result<ListSessionsResponse> {
        let mut url = self.url("/api/v1/sessions")?;
        // Omitted filters fall back to the router's defaults; only touch the query
        // string when at least one filter is set, to avoid a trailing `?`.
        if scope.is_some() || title.is_some() {
            let mut pairs = url.query_pairs_mut();
            if let Some(scope) = scope {
                pairs.append_pair("scope", &scope);
            }
            if let Some(title) = title {
                pairs.append_pair("title", &title);
            }
        }
        let builder = self.authorize(Request::get(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }

    async fn get_session(&self, id: Uuid) -> Result<GetSessionResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}"))?;
        let builder = self.authorize(Request::get(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }

    async fn update_session(
        &self,
        id: Uuid,
        req: UpdateSessionRequest,
    ) -> Result<UpdateSessionResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}"))?;
        let builder = self.authorize(Request::put(url.as_str()))?;
        self.send(json_request(builder, &req)?).await
    }

    async fn delete_session(&self, id: Uuid) -> Result<DeleteSessionResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}"))?;
        let builder = self.authorize(Request::delete(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }

    async fn execute(&self, id: Uuid, req: ExecuteRequest) -> Result<TurnResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}/execute"))?;
        let builder = self.authorize(Request::post(url.as_str()))?;
        let builder = self.provider_headers(builder);
        self.send(json_request(builder, &req)?).await
    }

    async fn continue_run(&self, id: Uuid, req: ContinueRequest) -> Result<TurnResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}/continue"))?;
        let builder = self.authorize(Request::post(url.as_str()))?;
        let builder = self.provider_headers(builder);
        self.send(json_request(builder, &req)?).await
    }

    async fn stream_execute(
        &self,
        id: Uuid,
        req: ExecuteRequest,
    ) -> Result<BoxStream<'static, Result<TurnEvent>>> {
        let url = self.url(&format!("/api/v1/sessions/{id}/execute"))?;
        let builder = self.authorize(Request::post(url.as_str()))?;
        let builder = self
            .provider_headers(builder)
            .header("accept", "text/event-stream");
        self.open_stream(json_request(builder, &req)?).await
    }

    async fn stream_continue(
        &self,
        id: Uuid,
        req: ContinueRequest,
    ) -> Result<BoxStream<'static, Result<TurnEvent>>> {
        let url = self.url(&format!("/api/v1/sessions/{id}/continue"))?;
        let builder = self.authorize(Request::post(url.as_str()))?;
        let builder = self
            .provider_headers(builder)
            .header("accept", "text/event-stream");
        self.open_stream(json_request(builder, &req)?).await
    }

    async fn preview(&self, id: Uuid, req: ExecuteRequest) -> Result<PreviewResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}/preview"))?;
        // Preview never calls a model, so it carries no provider credentials.
        let builder = self.authorize(Request::post(url.as_str()))?;
        self.send(json_request(builder, &req)?).await
    }

    async fn get_session_traces(
        &self,
        id: Uuid,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<TraceResponse> {
        let mut url = self.url(&format!("/api/v1/sessions/{id}/traces"))?;
        // Omitted bounds fall back to the router's defaults; only touch the query
        // string when at least one bound is set, to avoid a trailing `?`.
        if limit.is_some() || offset.is_some() {
            let mut pairs = url.query_pairs_mut();
            if let Some(limit) = limit {
                pairs.append_pair("limit", &limit.to_string());
            }
            if let Some(offset) = offset {
                pairs.append_pair("offset", &offset.to_string());
            }
        }
        let builder = self.authorize(Request::get(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }

    async fn list_providers(&self) -> Result<ListProvidersResponse> {
        let url = self.url("/api/v1/llm/providers")?;
        let builder = self.authorize(Request::get(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }

    async fn list_models(&self) -> Result<ListModelsResponse> {
        let url = self.url("/api/v1/llm/models")?;
        // Provider credentials let the router enumerate credentialed remotes.
        let builder = self.authorize(Request::get(url.as_str()))?;
        let builder = self.provider_headers(builder);
        self.send(empty_request(builder)?).await
    }

    async fn session_usage(&self, id: Uuid) -> Result<SessionUsageResponse> {
        let url = self.url(&format!("/api/v1/sessions/{id}/usage"))?;
        let builder = self.authorize(Request::get(url.as_str()))?;
        self.send(empty_request(builder)?).await
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use smista_core::api::{ApiErrorCode, TurnOutcome};
    use smista_core::model::Provider;
    use smista_mock_web_server::{
        Endpoint, MockRouter, Request as MockRequest, ResponseTemplate, api_error, defaults, sse,
    };

    use super::*;
    /// A representative [`ExecuteRequest`] used to exercise the turn endpoints.
    fn execute_request() -> ExecuteRequest {
        use smista_core::api::{
            Attachments, ExecutePolicy, LocalPreferences, TaskInput, Workspace,
        };
        use smista_core::policy::{
            ClassificationConfig, PrivacyPolicy, RoutingPolicy, ToolsConfig,
        };

        ExecuteRequest {
            input: TaskInput {
                text: "hello".to_string(),
                command: None,
                explicit_model: None,
            },
            workspace: Workspace {
                root: "/tmp".into(),
                git_branch: None,
                git_diff: None,
                referenced_paths: Vec::new(),
                active_file: None,
            },
            policy: ExecutePolicy {
                version: 1,
                source: "merged".to_string(),
                classification: ClassificationConfig::default(),
                routing: RoutingPolicy::default(),
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

    /// Builds a client pointed at `router` with no credentials held.
    fn client_for(router: &MockRouter) -> IsahcClient {
        IsahcClient::new(RouterClientConfig::new(router.base_url())).expect("the client builds")
    }

    /// Builds a client already holding a session token, by signing in first.
    async fn signed_in_client(router: &MockRouter) -> IsahcClient {
        let client = client_for(router);
        client
            .sign_in()
            .await
            .expect("sign-in succeeds against the mock");
        client
    }

    /// Returns the requests the mock recorded, in arrival order.
    async fn received(router: &MockRouter) -> Vec<MockRequest> {
        router.received_requests().await
    }

    /// Reads a header off the first recorded request whose path ends with `suffix`.
    fn header_of(requests: &[MockRequest], suffix: &str, name: &str) -> Option<String> {
        requests
            .iter()
            .find(|request| request.url.path().ends_with(suffix))
            .and_then(|request| request.headers.get(name))
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    #[test]
    fn should_get_base_url() {
        let url = Url::parse("https://example.com").expect("the URL parses");
        let client =
            IsahcClient::new(RouterClientConfig::new(url.clone())).expect("the client builds");
        assert_eq!(client.base_url(), &url);
    }

    #[tokio::test]
    async fn should_get_status_without_authentication() {
        let router = MockRouter::start().await;
        let status = client_for(&router).status().await.expect("status succeeds");
        assert_eq!(status, defaults::status());
    }

    #[tokio::test]
    async fn should_short_circuit_authenticated_calls_without_a_token() {
        let router = MockRouter::start().await;
        let client = client_for(&router);

        let error = client
            .list_sessions(None, None)
            .await
            .expect_err("a call without a token is rejected");
        assert!(matches!(error, RouterClientError::NotAuthenticated));
        // The guard returns before any request is sent.
        assert!(received(&router).await.is_empty());
    }

    #[tokio::test]
    async fn should_bootstrap_and_hold_the_minted_api_key() {
        let router = MockRouter::start().await;
        let client = client_for(&router);

        let response = client.bootstrap().await.expect("bootstrap succeeds");
        assert_eq!(response, defaults::bootstrap());

        // The held key is now presented on sign-in.
        client.sign_in().await.expect("sign-in succeeds");
        let requests = received(&router).await;
        assert_eq!(
            header_of(&requests, "/auth/sign-in", "x-smista-api-key").as_deref(),
            Some(defaults::bootstrap().api_key.as_str())
        );
    }

    #[tokio::test]
    async fn should_sign_in_and_authenticate_subsequent_calls() {
        let router = MockRouter::start().await;
        let client = signed_in_client(&router).await;

        client
            .list_sessions(None, None)
            .await
            .expect("an authenticated call succeeds");
        let requests = received(&router).await;
        let authorization = header_of(&requests, "/sessions", "authorization")
            .expect("the bearer header is present");
        assert!(authorization.starts_with("Bearer "));
    }

    #[tokio::test]
    async fn should_sign_out_and_clear_the_held_token() {
        let router = MockRouter::start().await;
        let client = signed_in_client(&router).await;

        let response = client.sign_out().await.expect("sign-out succeeds");
        assert!(response.revoked);

        // With the token cleared, the next authenticated call short-circuits.
        let error = client
            .list_sessions(None, None)
            .await
            .expect_err("the token was cleared");
        assert!(matches!(error, RouterClientError::NotAuthenticated));
    }

    #[tokio::test]
    async fn should_share_auth_state_across_clones() {
        let router = MockRouter::start().await;
        let client = client_for(&router);
        let clone = client.clone();

        // Signing in on one handle is visible through the other.
        client.sign_in().await.expect("sign-in succeeds");
        clone
            .list_sessions(None, None)
            .await
            .expect("the clone sees the shared token");
    }

    #[tokio::test]
    async fn should_seed_credentials_through_the_builders() {
        let router = MockRouter::start().await;
        let client = client_for(&router)
            .with_api_key(ApiKey::from_parts(&Uuid::nil(), "asd"))
            .with_session_token(SessionToken::from_parts(&Uuid::nil(), "seeded-token"))
            .with_provider_credentials(
                ProviderCredentials::new()
                    .with_provider(Provider::Anthropic, SecretString::from("sk-ant")),
            );

        // A seeded token authenticates without an explicit sign-in.
        client.me().await.expect("the seeded token authenticates");
    }

    #[tokio::test]
    async fn should_map_a_non_success_response_to_an_api_error() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::ListSessions,
                api_error(ApiErrorCode::InvalidToken, "bad token"),
            )
            .start()
            .await;
        let client = signed_in_client(&router).await;

        let error = client
            .list_sessions(None, None)
            .await
            .expect_err("the error body maps to an Api error");
        let RouterClientError::Api { status, code, .. } = error else {
            panic!("expected an Api error, got {error:?}");
        };
        assert_eq!(status, 401);
        assert_eq!(code, "invalid_token");
    }

    #[tokio::test]
    async fn should_map_a_malformed_body_to_a_decode_error() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::ListSessions,
                ResponseTemplate::new(200).set_body_string("not json"),
            )
            .start()
            .await;
        let client = signed_in_client(&router).await;

        let error = client
            .list_sessions(None, None)
            .await
            .expect_err("a malformed body fails to decode");
        assert!(matches!(error, RouterClientError::Decode(_)));
    }

    #[tokio::test]
    async fn should_forward_provider_credentials_on_execute() {
        let router = MockRouter::start().await;
        let client = client_for(&router).with_provider_credentials(
            ProviderCredentials::new()
                .with_provider(Provider::Anthropic, SecretString::from("sk-ant")),
        );
        client.sign_in().await.expect("sign-in succeeds");

        client
            .execute(Uuid::nil(), execute_request())
            .await
            .expect("execute succeeds");
        let requests = received(&router).await;
        assert_eq!(
            header_of(&requests, "/execute", "x-smista-provider-anthropic-api-key").as_deref(),
            Some("sk-ant")
        );
    }

    #[tokio::test]
    async fn should_forward_provider_credentials_on_list_models() {
        let router = MockRouter::start().await;
        let client = client_for(&router).with_provider_credentials(
            ProviderCredentials::new()
                .with_provider(Provider::Anthropic, SecretString::from("sk-ant")),
        );
        client.sign_in().await.expect("sign-in succeeds");

        client.list_models().await.expect("list_models succeeds");
        let requests = received(&router).await;
        assert_eq!(
            header_of(
                &requests,
                "/llm/models",
                "x-smista-provider-anthropic-api-key"
            )
            .as_deref(),
            Some("sk-ant")
        );
    }

    #[tokio::test]
    async fn should_not_forward_provider_credentials_on_preview() {
        let router = MockRouter::start().await;
        let client = client_for(&router).with_provider_credentials(
            ProviderCredentials::new()
                .with_provider(Provider::Anthropic, SecretString::from("sk-ant")),
        );
        client.sign_in().await.expect("sign-in succeeds");

        client
            .preview(Uuid::nil(), execute_request())
            .await
            .expect("preview succeeds");
        let requests = received(&router).await;
        assert!(
            header_of(&requests, "/preview", "x-smista-provider-anthropic-api-key").is_none(),
            "preview never calls a model, so it must carry no provider credentials"
        );
    }

    #[tokio::test]
    async fn should_send_pagination_as_query_parameters_for_traces() {
        let router = MockRouter::start().await;
        let client = signed_in_client(&router).await;

        client
            .get_session_traces(Uuid::nil(), Some(10), Some(5))
            .await
            .expect("traces succeed");
        let requests = received(&router).await;
        let query = requests
            .iter()
            .find(|request| request.url.path().ends_with("/traces"))
            .and_then(|request| request.url.query())
            .expect("the traces request carries a query string");
        assert!(query.contains("limit=10"));
        assert!(query.contains("offset=5"));
    }

    #[tokio::test]
    async fn should_send_scope_and_title_as_query_parameters_for_list_sessions() {
        let router = MockRouter::start().await;
        let client = signed_in_client(&router).await;

        client
            .list_sessions(
                Some("/home/me/project".to_string()),
                Some("auth".to_string()),
            )
            .await
            .expect("list_sessions succeeds");
        let requests = received(&router).await;
        let query = requests
            .iter()
            .find(|request| request.url.path().ends_with("/sessions"))
            .and_then(|request| request.url.query())
            .expect("the list request carries a query string");
        assert!(query.contains("scope="));
        assert!(query.contains("title=auth"));
    }

    #[tokio::test]
    async fn should_never_place_credentials_in_the_query_string() {
        let router = MockRouter::start().await;
        let client = client_for(&router).with_provider_credentials(
            ProviderCredentials::new()
                .with_provider(Provider::Anthropic, SecretString::from("sk-ant")),
        );
        client.sign_in().await.expect("sign-in succeeds");
        client
            .execute(Uuid::nil(), execute_request())
            .await
            .expect("execute succeeds");

        for request in received(&router).await {
            let query = request.url.query().unwrap_or_default();
            assert!(
                !query.contains("sk-ant"),
                "a provider key leaked into a query"
            );
            assert!(
                !query.to_ascii_lowercase().contains("api"),
                "an api credential leaked into a query: {query}"
            );
        }
    }

    #[tokio::test]
    async fn should_round_trip_every_authenticated_endpoint() {
        let router = MockRouter::start().await;
        let client = signed_in_client(&router).await;
        let id = Uuid::nil();

        assert_eq!(client.me().await.expect("me"), defaults::me());
        assert_eq!(
            client
                .create_session(smista_core::api::CreateSessionRequest {
                    title: "t".to_string(),
                    scope: None,
                    key_id: None,
                })
                .await
                .expect("create_session"),
            defaults::create_session()
        );
        assert_eq!(
            client
                .list_sessions(None, None)
                .await
                .expect("list_sessions"),
            defaults::list_sessions()
        );
        assert_eq!(
            client.get_session(id).await.expect("get_session"),
            defaults::get_session()
        );
        assert_eq!(
            client
                .update_session(id, smista_core::api::UpdateSessionRequest::default())
                .await
                .expect("update_session"),
            defaults::update_session()
        );
        assert_eq!(
            client.delete_session(id).await.expect("delete_session"),
            defaults::delete_session()
        );
        assert_eq!(
            client
                .execute(id, execute_request())
                .await
                .expect("execute"),
            defaults::turn()
        );
        assert_eq!(
            client
                .continue_run(id, smista_core::api::ContinueRequest::Break)
                .await
                .expect("continue_run"),
            defaults::turn()
        );
        assert_eq!(
            client
                .preview(id, execute_request())
                .await
                .expect("preview"),
            defaults::preview()
        );
        assert_eq!(
            client
                .get_session_traces(id, None, None)
                .await
                .expect("traces"),
            defaults::traces()
        );
        assert_eq!(
            client.list_providers().await.expect("list_providers"),
            defaults::list_providers()
        );
        assert_eq!(
            client.list_models().await.expect("list_models"),
            defaults::list_models()
        );
        assert_eq!(
            client.session_usage(id).await.expect("session_usage"),
            defaults::session_usage()
        );
    }

    #[tokio::test]
    async fn should_stream_a_turn_until_the_terminal_event() {
        let events = [
            TurnEvent::TextDelta {
                delta: "Hello".to_string(),
            },
            TurnEvent::TurnEnd(Box::new(defaults::turn())),
        ];
        let router = MockRouter::builder()
            .respond(Endpoint::Execute, sse(&events))
            .start()
            .await;
        let client = signed_in_client(&router).await;

        let collected: Vec<_> = client
            .stream_execute(Uuid::nil(), execute_request())
            .await
            .expect("the stream opens")
            .collect()
            .await;

        assert_eq!(collected.len(), 2);
        assert!(matches!(collected[0], Ok(TurnEvent::TextDelta { .. })));
        let last = collected[1].as_ref().expect("the terminal event decodes");
        assert!(matches!(last, TurnEvent::TurnEnd(_)));
    }

    #[tokio::test]
    async fn should_stream_a_continued_turn() {
        let events = [TurnEvent::TurnEnd(Box::new(defaults::turn()))];
        let router = MockRouter::builder()
            .respond(Endpoint::ContinueRun, sse(&events))
            .start()
            .await;
        let client = signed_in_client(&router).await;

        let collected: Vec<_> = client
            .stream_continue(Uuid::nil(), smista_core::api::ContinueRequest::Break)
            .await
            .expect("the stream opens")
            .collect()
            .await;
        assert_eq!(collected.len(), 1);
        assert!(matches!(
            collected[0].as_ref().expect("event decodes"),
            TurnEvent::TurnEnd(_)
        ));
    }

    #[tokio::test]
    async fn should_carry_an_error_outcome_on_the_terminal_event() {
        // Once a stream has opened the HTTP status is fixed, so a failed turn
        // rides the terminal `turn_end` as an error outcome rather than a status.
        let failed = TurnResponse {
            outcome: TurnOutcome::Error {
                error: smista_core::api::ApiErrorBody {
                    code: "provider_unavailable".to_string(),
                    message: "the provider is down".to_string(),
                    details: None,
                },
            },
            allowed_continuations: Vec::new(),
        };
        let events = [TurnEvent::TurnEnd(Box::new(failed))];
        let router = MockRouter::builder()
            .respond(Endpoint::Execute, sse(&events))
            .start()
            .await;
        let client = signed_in_client(&router).await;

        let collected: Vec<_> = client
            .stream_execute(Uuid::nil(), execute_request())
            .await
            .expect("the stream opens")
            .collect()
            .await;
        assert_eq!(collected.len(), 1);
        let TurnEvent::TurnEnd(turn) = collected[0].as_ref().expect("event decodes") else {
            panic!("expected a terminal event");
        };
        assert!(matches!(turn.outcome, TurnOutcome::Error { .. }));
    }

    #[tokio::test]
    async fn should_surface_a_failed_stream_open_as_an_error() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::Execute,
                api_error(ApiErrorCode::SessionNotFound, "no such session"),
            )
            .start()
            .await;
        let client = signed_in_client(&router).await;

        // The stream type is not `Debug`, so match instead of `expect_err`.
        let Err(error) = client.stream_execute(Uuid::nil(), execute_request()).await else {
            panic!("a non-success status must fail before the stream opens");
        };
        let RouterClientError::Api { code, .. } = error else {
            panic!("expected an Api error, got {error:?}");
        };
        assert_eq!(code, "session_not_found");
    }

    #[tokio::test]
    async fn should_surface_a_malformed_event_as_a_stream_error() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::Execute,
                ResponseTemplate::new(200)
                    .set_body_raw(b"data: not json\n\n".to_vec(), "text/event-stream"),
            )
            .start()
            .await;
        let client = signed_in_client(&router).await;

        let collected: Vec<_> = client
            .stream_execute(Uuid::nil(), execute_request())
            .await
            .expect("the stream opens")
            .collect()
            .await;
        assert_eq!(collected.len(), 1);
        assert!(matches!(collected[0], Err(RouterClientError::Decode(_))));
    }
}

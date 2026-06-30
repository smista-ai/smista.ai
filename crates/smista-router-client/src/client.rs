//! The [`Client`] trait: one method per `smista-router` endpoint.

#[cfg(feature = "isahc")]
mod isahc;
#[cfg(feature = "reqwest")]
mod reqwest;
#[cfg(feature = "ureq")]
mod ureq;

use std::future::Future;

use futures::stream::BoxStream;
use smista_core::api::{
    BootstrapResponse, ContinueRequest, CreateSessionRequest, CreateSessionResponse,
    DeleteSessionResponse, ExecuteRequest, GetSessionResponse, ListModelsResponse,
    ListProvidersResponse, ListSessionsResponse, MeResponse, PreviewResponse, SessionUsageResponse,
    SignInResponse, SignOutResponse, StatusResponse, TraceResponse, TurnEvent, TurnResponse,
    UpdateSessionRequest, UpdateSessionResponse,
};
use uuid::Uuid;

#[cfg(feature = "isahc")]
#[cfg_attr(docsrs, doc(cfg(feature = "isahc")))]
pub use self::isahc::IsahcClient;
#[cfg(feature = "reqwest")]
#[cfg_attr(docsrs, doc(cfg(feature = "reqwest")))]
pub use self::reqwest::ReqwestClient;
#[cfg(feature = "ureq")]
#[cfg_attr(docsrs, doc(cfg(feature = "ureq")))]
pub use self::ureq::UreqClient;
use crate::error::Result;

/// An async client for the `smista-router` HTTP JSON API.
///
/// The trait has one method per router endpoint — health, authentication,
/// sessions, execution, streaming, route preview, traces, providers and models,
/// and usage. Concrete clients (each backed by a different HTTP library) live in
/// separate crates; this trait is the backend-agnostic contract they share, so
/// frontends like `smista-cli` program against it rather than hand-rolling HTTP.
///
/// # Use through generics, not `dyn`
///
/// Methods return [`impl Future`](Future) (native return-position `impl Trait`),
/// which makes the trait **not** object safe: it cannot be used as `dyn Client`.
/// Consumers take a generic `C: Client` instead and pick one concrete client per
/// application:
///
/// ```
/// use smista_router_client::{Client, Result};
/// use smista_core::api::StatusResponse;
///
/// async fn check<C: Client>(client: &C) -> Result<StatusResponse> {
///     client.status().await
/// }
/// ```
///
/// The trait is `Send + Sync + 'static` and every returned future is `Send`, so a
/// client can be shared and driven on a multi-threaded runtime.
///
/// # State held by the client
///
/// Credentials are held by the concrete client, never passed per call.
/// [`bootstrap`](Self::bootstrap) and [`sign_in`](Self::sign_in) populate the
/// held session token, [`sign_out`](Self::sign_out) clears it, and every
/// authenticated method uses it. The provider keys the router needs to reach a
/// model (its [`ProviderCredentials`](crate::ProviderCredentials)) are configured
/// on the client too, so the methods that can call a model take no credential
/// argument either.
///
/// Every fallible method returns [`Result`](crate::Result), whose error is
/// [`RouterClientError`](crate::RouterClientError).
pub trait Client: Send + Sync + 'static {
    /// Calls the public `GET /status` health check, outside `/api/v1`.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) if the request
    /// cannot be sent or the response cannot be decoded.
    fn status(&self) -> impl Future<Output = Result<StatusResponse>> + Send;

    /// Calls `POST /api/v1/auth/bootstrap` to create the first user and mint its
    /// long-lived API key, returned exactly once.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) on transport
    /// failure, a non-success status, or a body that fails to decode.
    fn bootstrap(&self) -> impl Future<Output = Result<BootstrapResponse>> + Send;

    /// Calls `POST /api/v1/auth/sign-in`, exchanging the held API key for a
    /// session token the client then holds for authenticated calls.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) — an `Api` error
    /// with `invalid_api_key` or `missing_credentials` if the key is rejected or
    /// absent, or a transport or decode failure.
    fn sign_in(&self) -> impl Future<Output = Result<SignInResponse>> + Send;

    /// Calls `POST /api/v1/auth/sign-out`, revoking and clearing the held token.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, an `Api` error if the token is
    /// rejected, or a transport or decode failure.
    fn sign_out(&self) -> impl Future<Output = Result<SignOutResponse>> + Send;

    /// Calls `GET /api/v1/auth/me`, identifying the authenticated user from the
    /// held session token.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, an `Api` error if the token is
    /// rejected, or a transport or decode failure.
    fn me(&self) -> impl Future<Output = Result<MeResponse>> + Send;

    /// Calls `POST /api/v1/sessions` to create a session.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, an `Api` error for a rejected
    /// request, or a transport or decode failure.
    fn create_session(
        &self,
        req: CreateSessionRequest,
    ) -> impl Future<Output = Result<CreateSessionResponse>> + Send;

    /// Calls `GET /api/v1/sessions` to list the authenticated user's sessions.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, or a transport, status or decode
    /// failure.
    fn list_sessions(&self) -> impl Future<Output = Result<ListSessionsResponse>> + Send;

    /// Calls `GET /api/v1/sessions/{id}` to fetch a full session.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) — an `Api` error
    /// with `session_not_found` if the session does not exist or is not owned,
    /// `NotAuthenticated` if no token is held, or a transport or decode failure.
    fn get_session(&self, id: Uuid) -> impl Future<Output = Result<GetSessionResponse>> + Send;

    /// Calls `PUT /api/v1/sessions/{id}` to update a session's title and/or
    /// archive state, returning the updated summary.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) — an `Api` error
    /// with `session_not_found` for an unknown session, `NotAuthenticated` if no
    /// token is held, or a transport or decode failure.
    fn update_session(
        &self,
        id: Uuid,
        req: UpdateSessionRequest,
    ) -> impl Future<Output = Result<UpdateSessionResponse>> + Send;

    /// Calls `DELETE /api/v1/sessions/{id}` to delete a session and its content.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) — an `Api` error
    /// with `session_not_found` for an unknown session, `NotAuthenticated` if no
    /// token is held, or a transport or decode failure.
    fn delete_session(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<DeleteSessionResponse>> + Send;

    /// Calls `POST /api/v1/sessions/{id}/execute` and buffers the turn as a
    /// single [`TurnResponse`].
    ///
    /// The held provider credentials travel as request headers and are never
    /// persisted. For incremental delivery, use
    /// [`stream_execute`](Self::stream_execute) instead.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, an `Api` error for a rejected or
    /// failed run (for example `no_route` or `missing_provider_credentials`), or
    /// a transport or decode failure.
    fn execute(
        &self,
        id: Uuid,
        req: ExecuteRequest,
    ) -> impl Future<Output = Result<TurnResponse>> + Send;

    /// Calls `POST /api/v1/sessions/{id}/continue` to advance an in-flight run
    /// (tool results, approval decisions, decrypted records, injected input or an
    /// interrupt) and buffers the next turn as a [`TurnResponse`].
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, an `Api` error (for example
    /// `run_in_flight` or a continuation that does not answer the pause), or a
    /// transport or decode failure.
    fn continue_run(
        &self,
        id: Uuid,
        req: ContinueRequest,
    ) -> impl Future<Output = Result<TurnResponse>> + Send;

    /// Calls `POST /api/v1/sessions/{id}/execute` with `Accept: text/event-stream`
    /// and yields the turn as a stream of [`TurnEvent`]s, terminated by a
    /// `turn_end` carrying the final [`TurnResponse`].
    ///
    /// # Errors
    ///
    /// The outer future returns a [`RouterClientError`](crate::RouterClientError)
    /// if the stream cannot be opened (transport failure, a non-success status,
    /// or a missing token). Once open, each item is a [`Result`](crate::Result),
    /// so a mid-stream decode or transport error surfaces per event.
    fn stream_execute(
        &self,
        id: Uuid,
        req: ExecuteRequest,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<TurnEvent>>>> + Send;

    /// Calls `POST /api/v1/sessions/{id}/continue` with `Accept: text/event-stream`
    /// and yields the advanced turn as a stream of [`TurnEvent`]s, terminated by a
    /// `turn_end` carrying the final [`TurnResponse`].
    ///
    /// # Errors
    ///
    /// The outer future returns a [`RouterClientError`](crate::RouterClientError)
    /// if the stream cannot be opened. Once open, each item is a
    /// [`Result`](crate::Result), so a mid-stream error surfaces per event.
    fn stream_continue(
        &self,
        id: Uuid,
        req: ContinueRequest,
    ) -> impl Future<Output = Result<BoxStream<'static, Result<TurnEvent>>>> + Send;

    /// Calls `POST /api/v1/sessions/{id}/preview` to predict how a task would be
    /// routed without calling the model. Takes the same body as
    /// [`execute`](Self::execute).
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, an `Api` error if routing is
    /// rejected, or a transport or decode failure.
    fn preview(
        &self,
        id: Uuid,
        req: ExecuteRequest,
    ) -> impl Future<Output = Result<PreviewResponse>> + Send;

    /// Calls `GET /api/v1/sessions/{id}/traces`, a single paginated list windowed
    /// by `limit` and `offset` (each defaulting on the router when `None`).
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) — an `Api` error
    /// with `session_not_found` for an unknown session, `NotAuthenticated` if no
    /// token is held, or a transport or decode failure.
    fn get_session_traces(
        &self,
        id: Uuid,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> impl Future<Output = Result<TraceResponse>> + Send;

    /// Calls `GET /api/v1/llm/providers` to list the available providers.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, or a transport, status or decode
    /// failure.
    fn list_providers(&self) -> impl Future<Output = Result<ListProvidersResponse>> + Send;

    /// Calls `GET /api/v1/llm/models` to list the available models.
    ///
    /// The held provider credentials let the router query credentialed remote
    /// providers; a provider it cannot list is reported under
    /// [`ListModelsResponse::unavailable`](smista_core::api::ListModelsResponse::unavailable)
    /// rather than failing the call.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) —
    /// `NotAuthenticated` if no token is held, or a transport, status or decode
    /// failure.
    fn list_models(&self) -> impl Future<Output = Result<ListModelsResponse>> + Send;

    /// Calls `GET /api/v1/sessions/{id}/usage` for the session's token and cost
    /// breakdown.
    ///
    /// # Errors
    ///
    /// Returns a [`RouterClientError`](crate::RouterClientError) — an `Api` error
    /// with `session_not_found` for an unknown session, `NotAuthenticated` if no
    /// token is held, or a transport or decode failure.
    fn session_usage(&self, id: Uuid) -> impl Future<Output = Result<SessionUsageResponse>> + Send;
}

#[cfg(test)]
mod tests {
    use futures::{StreamExt as _, stream};

    use super::*;
    use crate::error::RouterClientError;

    /// A minimal in-memory [`Client`] used to prove the trait is implementable
    /// with native `async fn`, usable through generics, and that its futures are
    /// `Send`. It performs no I/O: `status` and the stream openers succeed with
    /// canned values, and the rest report [`RouterClientError::NotAuthenticated`]
    /// to avoid constructing every response type.
    struct MockClient;

    impl Client for MockClient {
        async fn status(&self) -> Result<StatusResponse> {
            Ok(StatusResponse {
                status: "ok".to_string(),
                version: "0.0.0".to_string(),
            })
        }

        async fn bootstrap(&self) -> Result<BootstrapResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn sign_in(&self) -> Result<SignInResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn sign_out(&self) -> Result<SignOutResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn me(&self) -> Result<MeResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn create_session(
            &self,
            _req: CreateSessionRequest,
        ) -> Result<CreateSessionResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn list_sessions(&self) -> Result<ListSessionsResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn get_session(&self, _id: Uuid) -> Result<GetSessionResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn update_session(
            &self,
            _id: Uuid,
            _req: UpdateSessionRequest,
        ) -> Result<UpdateSessionResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn delete_session(&self, _id: Uuid) -> Result<DeleteSessionResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn execute(&self, _id: Uuid, _req: ExecuteRequest) -> Result<TurnResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn continue_run(&self, _id: Uuid, _req: ContinueRequest) -> Result<TurnResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn stream_execute(
            &self,
            _id: Uuid,
            _req: ExecuteRequest,
        ) -> Result<BoxStream<'static, Result<TurnEvent>>> {
            Ok(stream::empty().boxed())
        }

        async fn stream_continue(
            &self,
            _id: Uuid,
            _req: ContinueRequest,
        ) -> Result<BoxStream<'static, Result<TurnEvent>>> {
            Ok(stream::once(async {
                Ok(TurnEvent::TextDelta {
                    delta: "hi".to_string(),
                })
            })
            .boxed())
        }

        async fn preview(&self, _id: Uuid, _req: ExecuteRequest) -> Result<PreviewResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn get_session_traces(
            &self,
            _id: Uuid,
            _limit: Option<u64>,
            _offset: Option<u64>,
        ) -> Result<TraceResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn list_providers(&self) -> Result<ListProvidersResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn list_models(&self) -> Result<ListModelsResponse> {
            Err(RouterClientError::NotAuthenticated)
        }

        async fn session_usage(&self, _id: Uuid) -> Result<SessionUsageResponse> {
            Err(RouterClientError::NotAuthenticated)
        }
    }

    /// Asserts `value` is `Send`; fails to compile otherwise.
    fn assert_send<T: Send>(_value: &T) {}

    /// A generic consumer, mirroring how applications use the trait: through a
    /// type parameter rather than `dyn Client`.
    async fn fetch_status<C: Client>(client: &C) -> Result<StatusResponse> {
        client.status().await
    }

    #[test]
    fn should_be_usable_through_generics_and_yield_send_futures() {
        let client = MockClient;

        // Every method's future must be `Send` so it can cross threads.
        assert_send(&client.status());
        assert_send(&client.list_sessions());
        assert_send(&client.execute(Uuid::nil(), dummy_execute()));
        assert_send(&client.stream_execute(Uuid::nil(), dummy_execute()));

        let status = futures::executor::block_on(fetch_status(&client))
            .expect("the mock status call succeeds");
        assert_eq!(status.status, "ok");
    }

    #[test]
    fn should_exercise_every_method() {
        // Drive each method once so the mock implementation, and therefore the
        // whole trait surface, is exercised rather than left as dead code.
        let client = MockClient;
        futures::executor::block_on(async {
            let _ = client.status().await;
            let _ = client.bootstrap().await;
            let _ = client.sign_in().await;
            let _ = client.sign_out().await;
            let _ = client.me().await;
            let _ = client
                .create_session(CreateSessionRequest {
                    title: "t".to_string(),
                    key_id: None,
                })
                .await;
            let _ = client.list_sessions().await;
            let _ = client.get_session(Uuid::nil()).await;
            let _ = client
                .update_session(Uuid::nil(), UpdateSessionRequest::default())
                .await;
            let _ = client.delete_session(Uuid::nil()).await;
            let _ = client.execute(Uuid::nil(), dummy_execute()).await;
            let _ = client
                .continue_run(Uuid::nil(), ContinueRequest::Break)
                .await;
            let _ = client.stream_execute(Uuid::nil(), dummy_execute()).await;
            let _ = client
                .stream_continue(Uuid::nil(), ContinueRequest::Break)
                .await;
            let _ = client.preview(Uuid::nil(), dummy_execute()).await;
            let _ = client
                .get_session_traces(Uuid::nil(), Some(10), Some(0))
                .await;
            let _ = client.list_providers().await;
            let _ = client.list_models().await;
            let _ = client.session_usage(Uuid::nil()).await;
        });
    }

    #[test]
    fn should_report_not_authenticated_for_protected_calls() {
        let client = MockClient;
        let error = futures::executor::block_on(client.list_sessions())
            .expect_err("the mock reports an unauthenticated state");
        assert!(matches!(error, RouterClientError::NotAuthenticated));
    }

    #[test]
    fn should_open_a_stream_and_yield_events() {
        let client = MockClient;
        let events = futures::executor::block_on(async {
            let stream = client
                .stream_continue(Uuid::nil(), ContinueRequest::Break)
                .await
                .expect("the mock opens the stream");
            stream.collect::<Vec<_>>().await
        });
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events.into_iter().next().unwrap(),
            Ok(TurnEvent::TextDelta { .. })
        ));
    }

    /// A minimal [`ExecuteRequest`] for exercising signatures.
    fn dummy_execute() -> ExecuteRequest {
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
}

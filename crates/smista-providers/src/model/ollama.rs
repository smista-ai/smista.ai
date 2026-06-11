//! Ollama model implementation.

use std::str::FromStr;
use std::sync::Arc;

use reqwest::header::HeaderName;
use rig_core::providers::ollama::{Client as OllamaClient, OllamaApiKey};
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::{ModelAuthRequirement, ModelDescriptor, ModelReference, Provider};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;

/// The default base URL of Ollama Cloud.
///
/// Used when an [`OllamaEndpoint`] is built with [`OllamaEndpoint::cloud`] and no
/// explicit host. Changing it redirects every authenticated request, so it must
/// stay in sync with Ollama's published cloud endpoint.
const OLLAMA_CLOUD_BASE_URL: &str = "https://ollama.com";

/// Where smista talks to Ollama: the connection's locality and base URL.
///
/// This is the **single source of truth** for an Ollama connection's locality.
/// Whether a model is `local` and its [`ModelAuthRequirement`] are derived from
/// the variant, and the base URL the prompts actually reach is fixed here, so a
/// model can never be advertised as local while its completions are routed to
/// the cloud — independent of the per-request credential.
///
/// Credentials are not part of the endpoint: they are supplied per request as an
/// [`Authentication`] when a model is resolved. A [`Local`](OllamaEndpoint::Local)
/// daemon is keyless, or fronted by a gateway that takes
/// [`Authentication::Headers`] credentials; a [`Cloud`](OllamaEndpoint::Cloud)
/// instance takes a bearer key.
///
/// # Examples
///
/// ```
/// use smista_providers::model::ollama::OllamaEndpoint;
///
/// // A local daemon: prompts never leave the machine.
/// let local = OllamaEndpoint::local("http://localhost:11434");
/// assert!(local.is_local());
///
/// // Ollama Cloud: not local.
/// let cloud = OllamaEndpoint::cloud();
/// assert!(!cloud.is_local());
/// ```
#[derive(Clone, Debug)]
pub enum OllamaEndpoint {
    /// A local or self-hosted Ollama daemon, e.g. `http://localhost:11434`.
    ///
    /// Treated as `local`: prompts sent to it never reach Ollama Cloud. A
    /// self-hosted instance fronted by a reverse proxy can still authenticate
    /// with header credentials supplied per request.
    Local {
        /// The base URL of the local daemon, e.g. `http://localhost:11434`.
        base_url: String,
    },
    /// Ollama Cloud: the public authenticated endpoint.
    ///
    /// Authentication is required (a bearer key supplied per request) and the
    /// model is **not** treated as `local`.
    Cloud {
        /// The base URL of the cloud instance.
        base_url: String,
    },
}

impl OllamaEndpoint {
    /// Creates a [`Local`](OllamaEndpoint::Local) endpoint for the given daemon URL.
    ///
    /// The endpoint is treated as `local`. A self-hosted instance behind a proxy
    /// that needs custom headers authenticates per request with
    /// [`Authentication::Headers`].
    pub fn local(base_url: impl Into<String>) -> Self {
        Self::Local {
            base_url: base_url.into(),
        }
    }

    /// Creates a [`Cloud`](OllamaEndpoint::Cloud) endpoint for Ollama Cloud.
    ///
    /// Uses `https://ollama.com` as the host; the bearer key is supplied per
    /// request as an [`Authentication`].
    pub fn cloud() -> Self {
        Self::Cloud {
            base_url: OLLAMA_CLOUD_BASE_URL.to_string(),
        }
    }

    /// Returns the base URL the endpoint connects to.
    pub fn base_url(&self) -> &str {
        match self {
            Self::Local { base_url } | Self::Cloud { base_url } => base_url,
        }
    }

    /// Returns `true` if this is a local daemon whose prompts never leave the host.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    /// Returns the authentication a model on this endpoint requires.
    ///
    /// [`None`](ModelAuthRequirement::None) for a local daemon,
    /// [`ApiKey`](ModelAuthRequirement::ApiKey) for cloud.
    pub fn auth_requirement(&self) -> ModelAuthRequirement {
        match self {
            Self::Local { .. } => ModelAuthRequirement::None,
            Self::Cloud { .. } => ModelAuthRequirement::ApiKey,
        }
    }
}

/// The non-connection runtime an [`OllamaModel`] needs to serve completions.
///
/// Connection facts (host, credentials, headers, locality) live in
/// [`OllamaEndpoint`]; this carries only the prompt preamble and the memory
/// backend, so the two concerns can never drift out of sync.
pub struct OllamaModelRuntime<S>
where
    S: MemoryStorage + 'static,
{
    /// The base system prompt; the model appends its memory preamble to this.
    pub preamble: String,
    /// The memory backend the model reads to build its preamble and writes
    /// through for the memory tool.
    pub storage: Arc<S>,
}

// `#[derive(Debug)]` would add a spurious `S: Debug` bound; render the storage
// type name by hand instead.
impl<S> std::fmt::Debug for OllamaModelRuntime<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaModelRuntime")
            .field("preamble", &self.preamble)
            .field(
                "storage",
                &format_args!("Arc<{}>", std::any::type_name::<S>()),
            )
            .finish()
    }
}

// `#[derive(Clone)]` would add a spurious `S: Clone` bound; `Arc<S>` is `Clone`
// for any `S`, so implement it by hand without the bound.
impl<S> Clone for OllamaModelRuntime<S>
where
    S: MemoryStorage + 'static,
{
    fn clone(&self) -> Self {
        Self {
            preamble: self.preamble.clone(),
            storage: Arc::clone(&self.storage),
        }
    }
}

/// The [`OllamaModel`] struct provides a way to dynamically construct models based on
/// those that are available on the ollama instance.
///
/// Since ollama can have models added and removed at runtime, and also user-defined, we can't have a static list of models.
///
/// Instead the [`crate::provider::ollama::OllamaProvider`] will query the ollama instance for the list of available models
/// and construct an [`OllamaModel`] for each of them, which will then be used to route requests to the correct model.
pub struct OllamaModel {
    agent: Agent<OllamaClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl OllamaModel {
    /// Creates a new Ollama model from an endpoint, runtime, authentication and
    /// descriptor.
    ///
    /// The base URL is read from `endpoint`, the single source of truth for
    /// locality, so the resolved model always talks to the same instance its
    /// `descriptor` was stamped against. The credentials — a bearer key or proxy
    /// headers — come from `authentication`. The preamble and memory backend
    /// come from `runtime`.
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] if the underlying client cannot be built or
    /// the agent fails to load its memory preamble.
    pub async fn new<S>(
        endpoint: &OllamaEndpoint,
        runtime: &OllamaModelRuntime<S>,
        authentication: &Authentication,
        descriptor: ModelDescriptor,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        let reference = descriptor.reference();
        tracing::debug!("Creating Ollama {reference} model");

        // `api_key` is a typestate transition: it changes the builder's type
        // from `ClientBuilder<.., Missing, ..>` to `ClientBuilder<.., OllamaApiKey, ..>`,
        // so the key must be set in a single chain rather than by reassigning a
        // `mut` binding (reassignment forces the key slot back to `Missing`). The
        // keyless branch feeds `Nothing`, which converts to `OllamaApiKey` via
        // `From<Nothing>`.
        let mut builder = OllamaClient::builder().base_url(endpoint.base_url());
        let api_key = match authentication.api_key() {
            Some(api_key) => OllamaApiKey::from(api_key.expose_secret()),
            None => OllamaApiKey::from(rig_core::client::Nothing),
        };

        // set extra headers, if any
        let extra_headers = authentication.headers();
        if !extra_headers.is_empty() {
            let headers = extra_headers.iter().fold(
                rig_core::http_client::HeaderMap::new(),
                |mut headers, (name, value)| {
                    let Ok(header_name) = HeaderName::from_str(name) else {
                        tracing::warn!(
                            "Failed to parse header name '{name}'; skipping this header",
                        );
                        return headers;
                    };
                    let Ok(value) = value.expose_secret().parse() else {
                        tracing::warn!(
                            "Failed to parse header value for '{name}'; skipping this header",
                        );
                        return headers;
                    };

                    headers.insert(header_name, value);
                    headers
                },
            );
            builder = builder.http_headers(headers);
        }

        // build client
        let client = builder.api_key(api_key).build().map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Ollama,
                Some(reference.model.clone()),
                "Failed to create Ollama client",
            )
        })?;

        tracing::debug!("Creating agent for Ollama {} model", reference.model);
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: reference.model.clone(),
            preamble: runtime.preamble.clone(),
            provider: Provider::Ollama,
            storage: Arc::clone(&runtime.storage),
        })
        .await?;

        Ok(Self {
            agent,
            descriptor,
            reference,
        })
    }
}

#[async_trait::async_trait]
impl Model for OllamaModel {
    fn reference(&self) -> &ModelReference {
        &self.reference
    }

    fn descriptor(&self) -> &ModelDescriptor {
        &self.descriptor
    }

    async fn complete(&self, request: CompletionRequest) -> ProviderResult<CompletionResponse> {
        self.agent.complete(request).await
    }

    async fn stream(&self, request: CompletionRequest) -> ProviderResult<ResponseStream> {
        self.agent.stream(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{MemoryRecord, MemoryStorage};

    /// Memory backend that stores nothing; used only to type the runtime.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(&self, _handle: String) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    #[test]
    fn local_endpoint_is_local_and_needs_no_auth() {
        let endpoint = OllamaEndpoint::local("http://localhost:11434");

        assert!(endpoint.is_local());
        assert_eq!(endpoint.base_url(), "http://localhost:11434");
        assert_eq!(endpoint.auth_requirement(), ModelAuthRequirement::None);
    }

    #[test]
    fn cloud_endpoint_is_not_local_and_requires_an_api_key() {
        let endpoint = OllamaEndpoint::cloud();

        assert!(!endpoint.is_local());
        assert_eq!(endpoint.base_url(), OLLAMA_CLOUD_BASE_URL);
        assert_eq!(endpoint.auth_requirement(), ModelAuthRequirement::ApiKey);
    }

    #[test]
    fn runtime_clone_shares_the_storage_handle() {
        let storage = Arc::new(NoStorage);
        let runtime = OllamaModelRuntime {
            preamble: "be helpful".to_string(),
            storage: Arc::clone(&storage),
        };

        let cloned = runtime.clone();

        assert_eq!(cloned.preamble, "be helpful");
        // The clone shares the same allocation rather than deep-copying storage.
        assert_eq!(Arc::strong_count(&storage), 3);
        assert!(Arc::ptr_eq(&runtime.storage, &cloned.storage));
    }
}

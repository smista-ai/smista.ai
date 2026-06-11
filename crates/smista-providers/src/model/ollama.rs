//! Ollama model implementation.

use std::str::FromStr;
use std::sync::Arc;

use reqwest::header::HeaderName;
use rig_core::providers::ollama::{Client as OllamaClient, OllamaApiKey};
use secrecy::{ExposeSecret, SecretString};
use smista_core::error::ProviderError;
use smista_core::model::{ModelAuthRequirement, ModelDescriptor, ModelReference, Provider};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::memory::MemoryStorage;
use crate::model::Model;

/// The default base URL of Ollama Cloud.
///
/// Used when an [`OllamaEndpoint`] is built with [`OllamaEndpoint::cloud`] and no
/// explicit host. Changing it redirects every authenticated request, so it must
/// stay in sync with Ollama's published cloud endpoint.
const OLLAMA_CLOUD_BASE_URL: &str = "https://ollama.com";

/// Where smista talks to Ollama, and how it authenticates.
///
/// This is the **single source of truth** for an Ollama connection's locality
/// and credentials. Every fact the rest of the adapter derives — the base URL,
/// the API key, the proxy headers, whether the model is `local`, and its
/// [`ModelAuthRequirement`] — comes from this one value, so a model can never be
/// advertised as local while its completions are routed to the cloud.
///
/// Locality is encoded in the variant rather than carried as a separate flag, so
/// the illegal "local instance with an API key" state is unrepresentable: a
/// [`Local`](OllamaEndpoint::Local) endpoint has no credential field to set.
///
/// # Examples
///
/// ```
/// use secrecy::SecretString;
/// use smista_providers::model::ollama::OllamaEndpoint;
///
/// // A local daemon: no API key, prompts never leave the machine.
/// let local = OllamaEndpoint::local("http://localhost:11434");
/// assert!(local.is_local());
///
/// // Ollama Cloud: authenticated, not local.
/// let cloud = OllamaEndpoint::cloud(SecretString::from("sk-ollama"));
/// assert!(!cloud.is_local());
/// ```
pub enum OllamaEndpoint {
    /// A local or self-hosted Ollama daemon, e.g. `http://localhost:11434`.
    ///
    /// Carries no Ollama Cloud API key and is treated as `local`: prompts sent
    /// to it never reach Ollama Cloud. A self-hosted instance fronted by a
    /// reverse proxy can still attach `extra_headers` (e.g. a proxy credential).
    Local {
        /// The base URL of the local daemon, e.g. `http://localhost:11434`.
        base_url: String,
        /// Extra headers for a custom local proxy, as (name, value) pairs.
        extra_headers: Vec<(String, SecretString)>,
    },
    /// Ollama Cloud: the public authenticated endpoint.
    ///
    /// Carries the bearer API key; authentication is required and the model is
    /// **not** treated as `local`.
    Cloud {
        /// The base URL of the cloud instance.
        base_url: String,
        /// The bearer API key sent as `Authorization: Bearer <api_key>`.
        api_key: SecretString,
    },
}

// Holds credentials (a cloud API key, or proxy header values), so `Debug` is
// implemented by hand to redact them rather than derived. A leak test in this
// module's `tests` guards the redaction (M-PUBLIC-DEBUG).
impl std::fmt::Debug for OllamaEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local {
                base_url,
                extra_headers,
            } => f
                .debug_struct("OllamaEndpoint::Local")
                .field("base_url", base_url)
                .field(
                    "extra_headers",
                    &format_args!("[{} header(s)]", extra_headers.len()),
                )
                .finish(),
            Self::Cloud { base_url, .. } => f
                .debug_struct("OllamaEndpoint::Cloud")
                .field("base_url", base_url)
                .field("api_key", &"[redacted]")
                .finish(),
        }
    }
}

// `#[derive(Clone)]` would work here, but is spelled out alongside the hand-written
// `Debug` to keep the credential-bearing fields' handling explicit.
impl Clone for OllamaEndpoint {
    fn clone(&self) -> Self {
        match self {
            Self::Local {
                base_url,
                extra_headers,
            } => Self::Local {
                base_url: base_url.clone(),
                extra_headers: extra_headers.clone(),
            },
            Self::Cloud { base_url, api_key } => Self::Cloud {
                base_url: base_url.clone(),
                api_key: api_key.clone(),
            },
        }
    }
}

impl OllamaEndpoint {
    /// Creates a [`Local`](OllamaEndpoint::Local) endpoint for the given daemon URL.
    ///
    /// The resulting endpoint carries no API key, no proxy headers, and is
    /// treated as `local`. Use [`local_with_headers`](OllamaEndpoint::local_with_headers)
    /// for a self-hosted instance behind a proxy that needs custom headers.
    pub fn local(base_url: impl Into<String>) -> Self {
        Self::Local {
            base_url: base_url.into(),
            extra_headers: Vec::new(),
        }
    }

    /// Creates a [`Local`](OllamaEndpoint::Local) endpoint with custom proxy headers.
    ///
    /// For a self-hosted instance fronted by a reverse proxy that requires extra
    /// headers (for example a proxy credential). The endpoint is still `local`.
    pub fn local_with_headers(
        base_url: impl Into<String>,
        extra_headers: Vec<(String, SecretString)>,
    ) -> Self {
        Self::Local {
            base_url: base_url.into(),
            extra_headers,
        }
    }

    /// Creates a [`Cloud`](OllamaEndpoint::Cloud) endpoint for Ollama Cloud.
    ///
    /// Uses `https://ollama.com` as the host and the given bearer API key.
    pub fn cloud(api_key: SecretString) -> Self {
        Self::Cloud {
            base_url: OLLAMA_CLOUD_BASE_URL.to_string(),
            api_key,
        }
    }

    /// Returns the base URL the endpoint connects to.
    pub fn base_url(&self) -> &str {
        match self {
            Self::Local { base_url, .. } | Self::Cloud { base_url, .. } => base_url,
        }
    }

    /// Returns the bearer API key, or [`None`] for a local endpoint.
    pub fn api_key(&self) -> Option<&SecretString> {
        match self {
            Self::Local { .. } => None,
            Self::Cloud { api_key, .. } => Some(api_key),
        }
    }

    /// Returns the extra proxy headers, empty for a cloud endpoint.
    pub fn extra_headers(&self) -> &[(String, SecretString)] {
        match self {
            Self::Local { extra_headers, .. } => extra_headers,
            Self::Cloud { .. } => &[],
        }
    }

    /// Returns `true` if this is a local daemon whose prompts never leave the host.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local { .. })
    }

    /// Returns the authentication a model on this endpoint requires.
    ///
    /// [`None`](ModelAuthRequirement::None) for a local daemon,
    /// [`ApiKey`](ModelAuthRequirement::ApiKey) for cloud or remote.
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
    /// Creates a new Ollama model from an endpoint, runtime and descriptor.
    ///
    /// The connection facts — base URL, API key and proxy headers — are read
    /// from `endpoint`, the single source of truth for locality and credentials,
    /// so the resolved model always talks to the same instance its `descriptor`
    /// was stamped against. The preamble and memory backend come from `runtime`.
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] if the underlying client cannot be built or
    /// the agent fails to load its memory preamble.
    pub async fn new<S>(
        endpoint: &OllamaEndpoint,
        runtime: &OllamaModelRuntime<S>,
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
        let api_key = match endpoint.api_key() {
            Some(api_key) => OllamaApiKey::from(api_key.expose_secret()),
            None => OllamaApiKey::from(rig_core::client::Nothing),
        };

        // set extra headers, if any
        let extra_headers = endpoint.extra_headers();
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
        assert!(endpoint.api_key().is_none());
        assert!(endpoint.extra_headers().is_empty());
        assert_eq!(endpoint.auth_requirement(), ModelAuthRequirement::None);
    }

    #[test]
    fn local_endpoint_carries_proxy_headers_but_no_api_key() {
        let endpoint = OllamaEndpoint::local_with_headers(
            "http://proxy.internal:11434",
            vec![("X-Proxy-Token".to_string(), SecretString::from("shhh"))],
        );

        assert!(endpoint.is_local());
        assert!(endpoint.api_key().is_none());
        assert_eq!(endpoint.extra_headers().len(), 1);
        assert_eq!(endpoint.extra_headers()[0].0, "X-Proxy-Token");
        assert_eq!(endpoint.auth_requirement(), ModelAuthRequirement::None);
    }

    #[test]
    fn cloud_endpoint_is_not_local_and_requires_an_api_key() {
        let endpoint = OllamaEndpoint::cloud(SecretString::from("sk-ollama-secret"));

        assert!(!endpoint.is_local());
        assert_eq!(endpoint.base_url(), OLLAMA_CLOUD_BASE_URL);
        assert_eq!(
            endpoint.api_key().map(ExposeSecret::expose_secret),
            Some("sk-ollama-secret")
        );
        assert!(endpoint.extra_headers().is_empty());
        assert_eq!(endpoint.auth_requirement(), ModelAuthRequirement::ApiKey);
    }

    #[test]
    fn debug_redacts_the_cloud_api_key() {
        let secret = "sk-ollama-9f8e7d6c5b4a";
        let endpoint = OllamaEndpoint::cloud(SecretString::from(secret));

        let rendered = format!("{endpoint:?}");

        assert!(rendered.contains("Cloud"));
        assert!(rendered.contains("[redacted]"));
        assert!(
            !rendered.contains(secret),
            "the API key leaked into Debug output"
        );
    }

    #[test]
    fn debug_redacts_local_proxy_header_values() {
        let header_secret = "proxy-bearer-1234";
        let endpoint = OllamaEndpoint::local_with_headers(
            "http://proxy.internal:11434",
            vec![(
                "X-Proxy-Token".to_string(),
                SecretString::from(header_secret),
            )],
        );

        let rendered = format!("{endpoint:?}");

        assert!(rendered.contains("Local"));
        assert!(rendered.contains("1 header(s)"));
        assert!(
            !rendered.contains(header_secret),
            "a proxy header value leaked into Debug output"
        );
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

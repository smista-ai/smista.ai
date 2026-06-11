//! OpenAI-compatible models: a [`Model`] that talks to any endpoint speaking
//! OpenAI's chat-completions wire format (vLLM, LM Studio, a gateway, …).
//!
//! Unlike the built-in [`openai`](crate::model::openai) adapter — which targets
//! OpenAI's own hosted API and ships a fixed catalog of GPT models — this
//! adapter is driven entirely by configuration: the base URL, optional
//! credential and per-model facts all come from a named instance declared by the
//! user. A single instance's connection lives in one [`OpenAICompatEndpoint`],
//! and the non-connection runtime (preamble + memory backend) in one
//! [`OpenAICompatRuntime`], so the two concerns never drift out of sync.

use std::sync::Arc;

use rig_core::client::BearerAuth;
use rig_core::providers::openai::client::Client as OpenAIClient;
use secrecy::{ExposeSecret, SecretString};
use smista_core::model::{ModelDescriptor, ModelReference};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::memory::MemoryStorage;
use crate::model::Model;

/// Bearer token sent to a keyless OpenAI-compatible endpoint.
///
/// rig's OpenAI client fixes its key type to [`BearerAuth`], which always emits
/// an `Authorization: Bearer …` header; unlike the Ollama client there is no
/// type that omits it (`Nothing` is rejected by the builder, and `Client` has no
/// public constructor that bypasses it). Keyless servers (local gateways,
/// self-hosted runtimes) ignore the header, so when no key is configured this
/// placeholder is sent purely to satisfy the type. Changing it has no effect on
/// authenticated endpoints, which always receive the configured key instead.
const PLACEHOLDER_API_KEY: &str = "no-key";

/// Where an OpenAI-compatible provider sends requests, and how it authenticates.
///
/// This is the **single source of truth** for an instance's connection: the
/// base URL and, optionally, the bearer credential. A keyless endpoint
/// ([`api_key`](OpenAICompatEndpoint::api_key) returns [`None`]) is for local or
/// self-hosted runtimes that ignore the `Authorization` header; a keyed endpoint
/// carries the token sent as `Authorization: Bearer <api_key>`.
///
/// # Examples
///
/// ```
/// use secrecy::SecretString;
/// use smista_providers::model::openai_compat::OpenAICompatEndpoint;
///
/// // A local vLLM server that needs no credential.
/// let local = OpenAICompatEndpoint::keyless("http://localhost:8000/v1");
/// assert!(local.api_key().is_none());
///
/// // A gateway behind a bearer token.
/// let gateway =
///     OpenAICompatEndpoint::keyed("https://gateway.internal/v1", SecretString::from("sk-x"));
/// assert!(gateway.api_key().is_some());
/// ```
#[derive(Clone)]
pub struct OpenAICompatEndpoint {
    base_url: String,
    api_key: Option<SecretString>,
}

// Holds a credential, so `Debug` is implemented by hand to redact the API key
// rather than derived. A leak test in this module's `tests` guards the
// redaction (M-PUBLIC-DEBUG).
impl std::fmt::Debug for OpenAICompatEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAICompatEndpoint")
            .field("base_url", &self.base_url)
            .field(
                "api_key",
                match self.api_key {
                    Some(_) => &"[redacted]",
                    None => &"None",
                },
            )
            .finish()
    }
}

impl OpenAICompatEndpoint {
    /// Creates a keyless endpoint for the given base URL.
    ///
    /// No credential is sent; intended for local or self-hosted runtimes that
    /// ignore the `Authorization` header.
    pub fn keyless(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: None,
        }
    }

    /// Creates a keyed endpoint that authenticates with the given bearer token.
    ///
    /// The token is sent as `Authorization: Bearer <api_key>`.
    pub fn keyed(base_url: impl Into<String>, api_key: SecretString) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: Some(api_key),
        }
    }

    /// Returns the base URL the endpoint connects to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Returns the bearer credential, or [`None`] for a keyless endpoint.
    pub fn api_key(&self) -> Option<&SecretString> {
        self.api_key.as_ref()
    }

    /// Returns the bearer token to send: the configured key, or the keyless
    /// [`PLACEHOLDER_API_KEY`] when none is set.
    fn bearer_token(&self) -> &str {
        self.api_key
            .as_ref()
            .map_or(PLACEHOLDER_API_KEY, ExposeSecret::expose_secret)
    }
}

/// The non-connection runtime an [`OpenAICompatModel`] needs to serve completions.
///
/// Connection facts (base URL, credential) live in [`OpenAICompatEndpoint`];
/// this carries only the prompt preamble and the memory backend, so the two
/// concerns can never drift out of sync. One runtime is shared across every
/// model a provider resolves.
pub struct OpenAICompatRuntime<S>
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
impl<S> std::fmt::Debug for OpenAICompatRuntime<S>
where
    S: MemoryStorage + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAICompatRuntime")
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
impl<S> Clone for OpenAICompatRuntime<S>
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

/// A model served by an OpenAI-compatible endpoint.
///
/// Adapts rig's OpenAI client — pointed at a custom base URL — to the
/// provider-agnostic [`Model`] interface. Its facts come from the
/// [`ModelDescriptor`] it is constructed with (assembled from the instance's
/// configuration), and its [`reference`](Model::reference) is derived from that
/// descriptor so the two can never disagree.
pub struct OpenAICompatModel {
    agent: Agent<OpenAIClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl OpenAICompatModel {
    /// Creates a new OpenAI-compatible model from an endpoint, runtime and descriptor.
    ///
    /// The connection facts — base URL and credential — are read from
    /// `endpoint`, the single source of truth for the instance, so the resolved
    /// model always talks to the instance its `descriptor` was stamped against.
    /// The preamble and memory backend come from `runtime`, and the model's
    /// stable identity is derived from `descriptor`.
    ///
    /// # Errors
    ///
    /// Returns a [`smista_core::error::ProviderError`] if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        endpoint: &OpenAICompatEndpoint,
        runtime: &OpenAICompatRuntime<S>,
        descriptor: ModelDescriptor,
    ) -> ProviderResult<Self>
    where
        S: MemoryStorage + 'static,
    {
        let reference = descriptor.reference();
        tracing::debug!("Creating OpenAI-compatible {reference} model");

        let client = OpenAIClient::builder()
            .base_url(endpoint.base_url())
            .api_key(BearerAuth::from(endpoint.bearer_token()))
            .build()
            .map_err(|e| {
                crate::error::provider_error(
                    crate::error::category_from_http(&e),
                    reference.provider.clone(),
                    Some(reference.model.clone()),
                    "Failed to create OpenAI-compatible client",
                )
            })?;

        tracing::debug!("Creating agent for OpenAI-compatible {reference} model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: reference.model.clone(),
            preamble: runtime.preamble.clone(),
            provider: reference.provider.clone(),
            storage: Arc::clone(&runtime.storage),
        })
        .await?;
        tracing::debug!("OpenAI-compatible {reference} model created successfully");

        Ok(Self {
            agent,
            descriptor,
            reference,
        })
    }
}

#[async_trait::async_trait]
impl Model for OpenAICompatModel {
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
    use std::sync::Arc;

    use secrecy::{ExposeSecret, SecretString};

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
    fn keyless_endpoint_sends_the_placeholder_token() {
        let endpoint = OpenAICompatEndpoint::keyless("http://localhost:8000/v1");

        assert_eq!(endpoint.base_url(), "http://localhost:8000/v1");
        assert!(endpoint.api_key().is_none());
        assert_eq!(endpoint.bearer_token(), PLACEHOLDER_API_KEY);
    }

    #[test]
    fn keyed_endpoint_sends_the_configured_token() {
        let endpoint = OpenAICompatEndpoint::keyed(
            "https://gateway.internal/v1",
            SecretString::from("sk-abc"),
        );

        assert_eq!(endpoint.base_url(), "https://gateway.internal/v1");
        assert_eq!(
            endpoint.api_key().map(ExposeSecret::expose_secret),
            Some("sk-abc")
        );
        assert_eq!(endpoint.bearer_token(), "sk-abc");
    }

    #[test]
    fn debug_redacts_the_endpoint_api_key() {
        let secret = "sk-compat-9f8e7d6c5b4a";
        let endpoint =
            OpenAICompatEndpoint::keyed("https://gateway.internal/v1", SecretString::from(secret));

        let rendered = format!("{endpoint:?}");

        assert!(rendered.contains("[redacted]"));
        assert!(
            !rendered.contains(secret),
            "the API key leaked into Debug output"
        );
    }

    #[test]
    fn debug_marks_a_keyless_endpoint_as_none() {
        let endpoint = OpenAICompatEndpoint::keyless("http://localhost:8000/v1");

        let rendered = format!("{endpoint:?}");

        assert!(rendered.contains("None"));
        assert!(!rendered.contains("[redacted]"));
    }

    #[test]
    fn endpoint_clone_preserves_base_url_and_key() {
        let endpoint = OpenAICompatEndpoint::keyed(
            "https://gateway.internal/v1",
            SecretString::from("sk-abc"),
        );

        let cloned = endpoint.clone();

        assert_eq!(cloned.base_url(), "https://gateway.internal/v1");
        assert_eq!(
            cloned.api_key().map(ExposeSecret::expose_secret),
            Some("sk-abc")
        );
    }

    #[test]
    fn runtime_clone_shares_the_storage_handle() {
        let storage = Arc::new(NoStorage);
        let runtime = OpenAICompatRuntime {
            preamble: "be helpful".to_string(),
            storage: Arc::clone(&storage),
        };

        let cloned = runtime.clone();

        assert_eq!(cloned.preamble, "be helpful");
        // The clone shares the same allocation rather than deep-copying storage.
        assert_eq!(Arc::strong_count(&storage), 3);
        assert!(Arc::ptr_eq(&runtime.storage, &cloned.storage));
    }

    #[test]
    fn runtime_debug_renders_the_storage_type_without_an_s_debug_bound() {
        let runtime = OpenAICompatRuntime {
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        };

        let rendered = format!("{runtime:?}");

        assert!(rendered.contains("OpenAICompatRuntime"));
        assert!(rendered.contains("be helpful"));
    }
}

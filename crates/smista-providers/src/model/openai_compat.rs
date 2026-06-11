//! OpenAI-compatible models: a [`Model`] that talks to any endpoint speaking
//! OpenAI's chat-completions wire format (vLLM, LM Studio, a gateway, …).
//!
//! Unlike the built-in [`openai`](crate::model::openai) adapter — which targets
//! OpenAI's own hosted API and ships a fixed catalog of GPT models — this
//! adapter is driven entirely by configuration: the base URL and per-model
//! facts all come from a named instance declared by the user. A single
//! instance's connection lives in one [`OpenAICompatEndpoint`], and the
//! non-connection runtime (preamble + memory backend) in one
//! [`OpenAICompatRuntime`], so the two concerns never drift out of sync. The
//! credential is not part of the instance: it is supplied per request as an
//! [`Authentication`] when a model is resolved.

use std::sync::Arc;

use rig_core::client::BearerAuth;
use rig_core::providers::openai::client::Client as OpenAIClient;
use secrecy::ExposeSecret;
use smista_core::model::{ModelDescriptor, ModelReference};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::auth::Authentication;
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

/// Where an OpenAI-compatible provider sends requests.
///
/// This is the **single source of truth** for an instance's connection: the
/// base URL. Credentials are not held here; they are supplied per request as an
/// [`Authentication`] when a model is resolved. A keyless endpoint is given
/// [`Authentication::None`] and sends the keyless placeholder token a keyless
/// server ignores; a keyed endpoint is given an [`Authentication::ApiKey`] sent
/// as `Authorization: Bearer <api_key>`.
///
/// # Examples
///
/// ```
/// use smista_providers::model::openai_compat::OpenAICompatEndpoint;
///
/// let endpoint = OpenAICompatEndpoint::new("http://localhost:8000/v1");
/// assert_eq!(endpoint.base_url(), "http://localhost:8000/v1");
/// ```
#[derive(Clone, Debug)]
pub struct OpenAICompatEndpoint {
    base_url: String,
}

impl OpenAICompatEndpoint {
    /// Creates an endpoint for the given base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Returns the base URL the endpoint connects to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Returns the bearer token to send for `authentication`: the supplied API key,
/// or the keyless [`PLACEHOLDER_API_KEY`] when none was supplied.
fn bearer_token(authentication: &Authentication) -> &str {
    authentication
        .api_key()
        .map_or(PLACEHOLDER_API_KEY, ExposeSecret::expose_secret)
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
    /// Creates a new OpenAI-compatible model from an endpoint, runtime,
    /// authentication and descriptor.
    ///
    /// The base URL is read from `endpoint`, the single source of truth for the
    /// instance, so the resolved model always talks to the instance its
    /// `descriptor` was stamped against. The bearer credential comes from
    /// `authentication` (or the keyless placeholder when none is supplied). The
    /// preamble and memory backend come from `runtime`, and the model's stable
    /// identity is derived from `descriptor`.
    ///
    /// # Errors
    ///
    /// Returns a [`smista_core::error::ProviderError`] if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        endpoint: &OpenAICompatEndpoint,
        runtime: &OpenAICompatRuntime<S>,
        authentication: &Authentication,
        descriptor: ModelDescriptor,
    ) -> ProviderResult<Self>
    where
        S: MemoryStorage + 'static,
    {
        let reference = descriptor.reference();
        tracing::debug!("Creating OpenAI-compatible {reference} model");

        let client = OpenAIClient::builder()
            .base_url(endpoint.base_url())
            .api_key(BearerAuth::from(bearer_token(authentication)))
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

    use secrecy::SecretString;

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
    fn keyless_authentication_sends_the_placeholder_token() {
        assert_eq!(bearer_token(&Authentication::None), PLACEHOLDER_API_KEY);
    }

    #[test]
    fn keyed_authentication_sends_the_configured_token() {
        let authentication = Authentication::ApiKey(SecretString::from("sk-abc"));

        assert_eq!(bearer_token(&authentication), "sk-abc");
    }

    #[test]
    fn header_authentication_falls_back_to_the_placeholder_token() {
        // The OpenAI-compatible adapter only understands a bearer key; a headers
        // variant carries no key, so the keyless placeholder is sent.
        let authentication =
            Authentication::Headers(vec![("X-Token".to_string(), SecretString::from("shhh"))]);

        assert_eq!(bearer_token(&authentication), PLACEHOLDER_API_KEY);
    }

    #[test]
    fn endpoint_clone_preserves_base_url() {
        let endpoint = OpenAICompatEndpoint::new("https://gateway.internal/v1");

        let cloned = endpoint.clone();

        assert_eq!(cloned.base_url(), "https://gateway.internal/v1");
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

//! Ollama [`Provider`] implementation.

pub mod client;

use std::sync::Arc;
use std::time::Duration;

use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::{
    ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference, Provider as ProviderId,
    ProviderDescriptor,
};

use self::client::{HttpOllamaClient, OllamaClient};
use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::{MemoryScope, MemoryStorage};
use crate::model::Model;
use crate::model::ollama::{OllamaEndpoint, OllamaModel, OllamaModelRuntime};
use crate::provider::cache::{Cached, TtlCache};

/// Type alias for the Ollama models cache, owned privately by the provider.
type ModelsCache = TtlCache<Vec<ModelDescriptor>>;

/// How long the Ollama models cache stays fresh before the daemon is queried
/// again.
///
/// The provider is long-lived and owns its cache, so this bounds how stale the
/// advertised model list can be while keeping repeat lookups off the daemon.
const MODELS_CACHE_TTL: Duration = Duration::from_secs(60);

/// A [`Provider`] backed by a single Ollama instance.
///
/// Offers the Ollama models the instance has installed and resolves a
/// [`ModelReference`] into an executable [`Model`] built from the shared
/// [`OllamaModelRuntime`]. Connection facts and locality come from a single
/// [`OllamaEndpoint`]: both the management client and every resolved model are
/// derived from it, so a model can never be advertised as local while its
/// completions are routed to the cloud. Credentials are not held here; they are
/// supplied per request as an [`Authentication`] when listing or resolving, so
/// one long-lived provider can serve many callers and owns its own model cache.
pub struct OllamaProvider<C, S>
where
    C: OllamaClient + 'static,
    S: MemoryStorage + 'static,
{
    /// [`OllamaClient`] used to query the instance's management endpoints.
    client: C,
    /// The single source of truth for the instance's host and locality.
    endpoint: OllamaEndpoint,
    /// Runtime (preamble + memory backend) used to construct each resolved [`OllamaModel`].
    runtime: OllamaModelRuntime<S>,
    /// Available models cache, refreshed with TTL.
    ///
    /// Owned by this provider: the provider is long-lived, so the cache lives
    /// with it rather than being shared in from outside.
    models: ModelsCache,
}

impl<S> OllamaProvider<HttpOllamaClient, S>
where
    S: MemoryStorage + 'static,
{
    /// Constructs a new [`OllamaProvider`] from an endpoint and runtime.
    ///
    /// The management [`HttpOllamaClient`] is built from `endpoint` via
    /// [`HttpOllamaClient::from_endpoint`], so the client and every resolved
    /// model provably target the same instance — closing the gap where a
    /// local-labelled model could route prompts to the cloud.
    ///
    /// The provider owns its own models cache, created with a fixed TTL, so a
    /// single long-lived instance keeps repeat lookups off the daemon.
    pub fn new(endpoint: OllamaEndpoint, runtime: OllamaModelRuntime<S>) -> Self {
        let client = HttpOllamaClient::from_endpoint(&endpoint);
        Self::with_client(client, endpoint, runtime)
    }
}

impl<C, S> OllamaProvider<C, S>
where
    C: OllamaClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Constructs a provider with an explicitly supplied management client.
    ///
    /// This is a testing and advanced seam: production code should use
    /// [`OllamaProvider::new`], which derives the client from the endpoint. The
    /// locality fact stamped onto every model is taken from `endpoint`, never
    /// from `client`, so even a mismatched client cannot make a local-labelled
    /// model complete against the cloud — the client only fetches the list of
    /// installed models. The provider owns a fresh models cache.
    pub fn with_client(
        client: C,
        endpoint: OllamaEndpoint,
        runtime: OllamaModelRuntime<S>,
    ) -> Self {
        Self {
            client,
            endpoint,
            runtime,
            models: ModelsCache::new(MODELS_CACHE_TTL),
        }
    }

    /// Fetches the available models from the Ollama instance, using the cache if valid.
    async fn fetch_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelDescriptor>> {
        if let Cached::Hit(cached) = self.models.get() {
            tracing::debug!("Ollama models cache hit: {} model(s)", cached.len());
            return Ok(cached);
        }

        tracing::debug!("Ollama models cache miss, fetching from instance");
        let tags = self.client.tags(authentication).await?;
        let models: Vec<ModelDescriptor> = tags
            .models
            .into_iter()
            .map(|model| self.normalize_tag_reference(model))
            .collect();

        tracing::debug!("Fetched {} model(s) from Ollama instance", models.len());
        self.models.set(models.clone());

        Ok(models)
    }

    fn normalize_tag_reference(&self, model: client::api::Model) -> ModelDescriptor {
        // Ollama serves every model over `/api/chat`, which universally supports
        // streaming, a system prompt, and `format`-constrained JSON output.
        // These are daemon features the tag list does not report per model, so
        // they are seeded on, mirroring the static remote adapters; the per-tag
        // capabilities below only add image, reasoning and tool support.
        let capabilities = model.capabilities.into_iter().fold(
            ModelCapabilities {
                streaming: true,
                system_prompt: true,
                json_output: true,
                ..Default::default()
            },
            |mut caps, cap| {
                match cap {
                    // Ollama reports image input as `vision`; `image` is accepted
                    // defensively in case the daemon ever uses that spelling.
                    client::api::Capability::Vision | client::api::Capability::Image => {
                        caps.images = true;
                    }
                    client::api::Capability::Thinking => caps.reasoning = true,
                    // Tool calling also satisfies the memory capability: the
                    // memory tool is driven by tool calls, so the router can run
                    // memory orchestration whenever a model can call tools.
                    client::api::Capability::Tools => {
                        caps.tools = true;
                        caps.memory = true;
                    }
                    client::api::Capability::Completion
                    | client::api::Capability::Embedding
                    | client::api::Capability::Insert
                    | client::api::Capability::Audio
                    | client::api::Capability::Other(_) => {}
                }
                caps
            },
        );

        // A locally served model costs nothing per token; the pricing of a
        // remote Ollama endpoint is unknown, so it stays unreported there.
        let token_cost = self
            .endpoint
            .is_local()
            .then_some(rust_decimal::Decimal::ZERO);

        ModelDescriptor {
            provider: smista_core::model::Provider::Ollama,
            model: model.model,
            display_name: Some(model.name),
            local: self.endpoint.is_local(),
            auth: self.endpoint.auth_requirement(),
            capabilities,
            max_context_tokens: model.details.context_length,
            max_output_tokens: None,
            input_cost_per_million_tokens: token_cost,
            output_cost_per_million_tokens: token_cost,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }
}

#[async_trait::async_trait]
impl<C, S> Provider for OllamaProvider<C, S>
where
    C: OllamaClient + 'static,
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::Ollama
    }

    fn descriptor(&self) -> ProviderDescriptor {
        // Locality comes from the endpoint, the same source that stamps every
        // resolved model's `local` flag, so the two can never disagree.
        ProviderDescriptor {
            id: ProviderId::Ollama,
            display_name: "Ollama".to_string(),
            local: self.endpoint.is_local(),
        }
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
        scope: MemoryScope,
    ) -> ProviderResult<Arc<dyn Model>> {
        let models = self.fetch_models(authentication).await?;

        tracing::debug!(
            "Resolving model reference {reference} against {} available models",
            models.len()
        );
        let descriptor = models
            .into_iter()
            .find(|model| &model.reference() == reference)
            .ok_or_else(|| ProviderError {
                message: format!("Could not find model {reference}"),
                provider: self.id(),
                category: ProviderErrorCategory::ModelNotFound,
                model: Some(reference.model.clone()),
            })?;

        tracing::debug!("Found model descriptor for {reference}: {descriptor:#?}; returning model",);
        Ok(Arc::new(
            OllamaModel::new(
                &self.endpoint,
                &self.runtime,
                authentication,
                descriptor,
                scope,
            )
            .await?,
        ))
    }

    async fn list_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>> {
        self.fetch_models(authentication)
            .await
            .map(|models| models.into_iter().map(|m| m.reference()).collect())
    }
}

#[cfg(test)]
mod tests {
    use smista_core::model::{Capability, ModelAuthRequirement};
    use uuid::Uuid;

    use super::client::MockOllamaClient;
    use super::client::api::{
        Capability as ApiCapability, Details, Model as ApiModel, TagsResponse,
    };
    use super::*;

    /// A throwaway memory scope; resolution does not bake it into the provider.
    fn scope() -> MemoryScope {
        MemoryScope {
            user_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
        }
    }
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; resolution paths under test never
    /// touch it.
    struct NoStorage;

    impl MemoryStorage for NoStorage {
        type Error = std::convert::Infallible;

        async fn put_user_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_user_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_user_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_user_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }

        async fn put_session_memory(
            &self,
            _scope: MemoryScope,
            _key: Option<String>,
            _content: String,
        ) -> Result<MemoryRecord, Self::Error> {
            unreachable!("not exercised by these tests")
        }

        async fn forget_session_memory(
            &self,
            _scope: MemoryScope,
            _handle: String,
        ) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_session_memories(
            &self,
            _scope: MemoryScope,
            _limit: Option<usize>,
        ) -> Result<Vec<MemoryRecord>, Self::Error> {
            Ok(Vec::new())
        }

        async fn get_session_memory_by_key(
            &self,
            _scope: MemoryScope,
            _key: String,
        ) -> Result<Option<MemoryRecord>, Self::Error> {
            Ok(None)
        }
    }

    fn model(name: &str, capabilities: Vec<ApiCapability>) -> ApiModel {
        ApiModel {
            name: name.to_string(),
            details: Details {
                context_length: 32768,
            },
            model: name.to_string(),
            capabilities,
        }
    }

    fn runtime() -> OllamaModelRuntime<NoStorage> {
        OllamaModelRuntime {
            preamble: "be helpful".to_string(),
            storage: Arc::new(NoStorage),
        }
    }

    fn provider_with(
        endpoint: OllamaEndpoint,
        models: Vec<ApiModel>,
    ) -> OllamaProvider<MockOllamaClient, NoStorage> {
        let client = MockOllamaClient::new(Ok(TagsResponse { models }));
        OllamaProvider::with_client(client, endpoint, runtime())
    }

    fn authentication() -> Authentication {
        Authentication::None
    }

    #[test]
    fn should_identify_as_ollama() {
        let provider = provider_with(OllamaEndpoint::local("http://localhost:11434"), vec![]);
        assert_eq!(provider.id(), ProviderId::Ollama);
    }

    #[test]
    fn should_seed_streaming_system_prompt_and_json_output_by_default() {
        let provider = provider_with(OllamaEndpoint::local("http://localhost:11434"), vec![]);

        // A model that reports only `completion` still streams, takes a system
        // prompt, and honours `format`-constrained JSON output.
        let caps = provider
            .normalize_tag_reference(model("llama3:8b", vec![ApiCapability::Completion]))
            .capabilities;

        assert!(caps.supports(Capability::Streaming));
        assert!(caps.supports(Capability::SystemPrompt));
        assert!(caps.supports(Capability::JsonOutput));
        // Per-tag capabilities are not granted unless reported.
        assert!(!caps.supports(Capability::Images));
        assert!(!caps.supports(Capability::Reasoning));
        assert!(!caps.supports(Capability::Tools));
        assert!(!caps.supports(Capability::Memory));
    }

    #[test]
    fn should_map_vision_to_images_and_thinking_to_reasoning() {
        let provider = provider_with(OllamaEndpoint::local("http://localhost:11434"), vec![]);

        let caps = provider
            .normalize_tag_reference(model(
                "devstral-small-2:latest",
                vec![
                    ApiCapability::Vision,
                    ApiCapability::Thinking,
                    // Capabilities the adapter does not model are dropped.
                    ApiCapability::Completion,
                    ApiCapability::Insert,
                    ApiCapability::Embedding,
                ],
            ))
            .capabilities;

        assert!(caps.supports(Capability::Images));
        assert!(caps.supports(Capability::Reasoning));
        assert!(!caps.supports(Capability::Tools));
    }

    #[test]
    fn should_derive_memory_from_tool_support() {
        let provider = provider_with(OllamaEndpoint::local("http://localhost:11434"), vec![]);

        let caps = provider
            .normalize_tag_reference(model("qwen2.5-coder:7b", vec![ApiCapability::Tools]))
            .capabilities;

        assert!(caps.supports(Capability::Tools));
        // Tool calling drives the memory tool, so memory follows.
        assert!(caps.supports(Capability::Memory));
    }

    #[test]
    fn should_stamp_local_descriptor_from_a_local_endpoint() {
        let provider = provider_with(OllamaEndpoint::local("http://localhost:11434"), vec![]);

        let descriptor = provider.normalize_tag_reference(model("llama3:8b", vec![]));

        assert!(descriptor.local);
        assert_eq!(descriptor.auth, ModelAuthRequirement::None);
    }

    #[test]
    fn should_stamp_cloud_descriptor_from_a_cloud_endpoint() {
        // Anti-leak: locality and auth come from the endpoint, never the daemon.
        let provider = provider_with(OllamaEndpoint::cloud(), vec![]);

        let descriptor = provider.normalize_tag_reference(model("llama3:8b", vec![]));

        assert!(!descriptor.local);
        assert_eq!(descriptor.auth, ModelAuthRequirement::ApiKey);
    }

    #[tokio::test]
    async fn should_list_installed_models() {
        let provider = provider_with(
            OllamaEndpoint::local("http://localhost:11434"),
            vec![
                model("llama3:8b", vec![ApiCapability::Tools]),
                model("qwen2.5-coder:7b", vec![ApiCapability::Completion]),
            ],
        );

        let listed = provider
            .list_models(&authentication())
            .await
            .expect("listing cannot fail");

        let names: Vec<&str> = listed.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(names, vec!["llama3:8b", "qwen2.5-coder:7b"]);
    }

    #[tokio::test]
    async fn should_serve_repeat_lookups_from_the_cache() {
        let provider = provider_with(
            OllamaEndpoint::local("http://localhost:11434"),
            vec![model("llama3:8b", vec![])],
        );

        provider
            .list_models(&authentication())
            .await
            .expect("first listing");
        provider
            .list_models(&authentication())
            .await
            .expect("second listing");

        // The instance is queried once; the hot cache serves the second lookup.
        assert_eq!(provider.client.tags_calls(), 1);
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let provider = provider_with(
            OllamaEndpoint::local("http://localhost:11434"),
            vec![model("llama3:8b", vec![])],
        );

        let unknown = ModelReference {
            provider: ProviderId::Ollama,
            model: "does-not-exist:1b".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider.resolve(&unknown, &authentication(), scope()).await else {
            panic!("an uninstalled model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::Ollama);
        assert_eq!(error.model.as_deref(), Some("does-not-exist:1b"));
    }

    #[tokio::test]
    async fn should_resolve_an_installed_model() {
        let provider = provider_with(
            OllamaEndpoint::local("http://localhost:11434"),
            vec![model("llama3:8b", vec![ApiCapability::Tools])],
        );

        let reference = ModelReference {
            provider: ProviderId::Ollama,
            model: "llama3:8b".to_string(),
        };

        let resolved = provider
            .resolve(&reference, &authentication(), scope())
            .await;
        assert!(resolved.is_ok(), "an installed model must resolve");
    }
}

//! Anthropic [`Provider`] implementation.

pub mod client;

use std::sync::Arc;
use std::time::Duration;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    Provider as ProviderId,
};

use self::client::{AnthropicClient, HttpAnthropicClient};
use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::anthropic::{self, AnthropicModel, AnthropicModelArgs};
use crate::provider::cache::{Cached, TtlCache};

/// How long the Anthropic models cache stays fresh before the API is queried
/// again.
///
/// The Anthropic catalog changes rarely, so a long, fixed time-to-live keeps
/// repeat lookups off the API while bounding how stale the advertised list can
/// be. One hour.
const MODELS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// A [`Provider`] backed by a single Anthropic account.
///
/// Offers the Claude models the account can see (fetched from `/v1/models` and
/// cached) and resolves a [`ModelReference`] into an executable [`Model`] built
/// from the shared [`AnthropicModelArgs`]. The credential is not held by the
/// provider: it is supplied per request as an [`Authentication`] when listing or
/// resolving, so one long-lived provider can serve many callers and owns its own
/// model cache.
pub struct AnthropicProvider<C, S>
where
    C: AnthropicClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Client used to query the Anthropic management endpoints.
    client: C,
    /// Shared arguments used to construct each resolved [`AnthropicModel`].
    args: AnthropicModelArgs<S>,
    /// Available models cache, refreshed with TTL.
    models: TtlCache<Vec<ModelDescriptor>>,
}

impl<S> AnthropicProvider<HttpAnthropicClient, S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`AnthropicProvider`] targeting the public Anthropic API.
    ///
    /// The provider owns its own models cache, created with a fixed TTL, so a
    /// single long-lived instance keeps repeat lookups off the API.
    pub fn new(args: AnthropicModelArgs<S>) -> Self {
        Self::with_client(HttpAnthropicClient::new(), args)
    }
}

impl<S> From<AnthropicModelArgs<S>> for AnthropicProvider<HttpAnthropicClient, S>
where
    S: MemoryStorage + 'static,
{
    fn from(args: AnthropicModelArgs<S>) -> Self {
        Self::new(args)
    }
}

impl<C, S> AnthropicProvider<C, S>
where
    C: AnthropicClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Constructs a provider with an explicitly supplied management client.
    ///
    /// This is a testing and advanced seam: production code should use
    /// [`AnthropicProvider::new`]. The provider owns a fresh models cache.
    pub fn with_client(client: C, args: AnthropicModelArgs<S>) -> Self {
        Self {
            client,
            args,
            models: TtlCache::new(MODELS_CACHE_TTL),
        }
    }

    /// Fetches the available models, using the cache when it is still fresh.
    async fn fetch_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelDescriptor>> {
        if let Cached::Hit(cached) = self.models.get() {
            tracing::debug!("Anthropic models cache hit: {} model(s)", cached.len());
            return Ok(cached);
        }

        tracing::debug!("Anthropic models cache miss, fetching from API");
        let models = self.client.models(authentication).await?;
        let descriptors: Vec<ModelDescriptor> =
            models.into_iter().map(Self::normalize_model).collect();

        tracing::debug!("Fetched {} model(s) from Anthropic API", descriptors.len());
        self.models.set(descriptors.clone());

        Ok(descriptors)
    }

    /// Turns an API model entry into a [`ModelDescriptor`].
    ///
    /// Streaming, system-prompt, tools, and memory are universal Anthropic
    /// features the listing does not flag per model, so they are seeded on; the
    /// remaining capabilities and the context sizes come from the entry, and the
    /// price comes from the family table.
    fn normalize_model(model: client::api::Model) -> ModelDescriptor {
        let capabilities = ModelCapabilities {
            streaming: true,
            system_prompt: true,
            tools: true,
            memory: true,
            json_output: model.capabilities.structured_outputs.supported,
            images: model.capabilities.image_input.supported,
            reasoning: model.capabilities.thinking.supported,
        };

        let (input_cost, output_cost) = anthropic::family_pricing(&model.id);

        ModelDescriptor {
            provider: ProviderId::Anthropic,
            model: model.id,
            display_name: model.display_name,
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities,
            max_context_tokens: model.max_input_tokens,
            max_output_tokens: model.max_tokens,
            input_cost_per_million_tokens: input_cost,
            output_cost_per_million_tokens: output_cost,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }
}

#[async_trait::async_trait]
impl<C, S> Provider for AnthropicProvider<C, S>
where
    C: AnthropicClient + 'static,
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::Anthropic
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
    ) -> ProviderResult<Arc<dyn Model>> {
        let descriptor = self
            .fetch_models(authentication)
            .await?
            .into_iter()
            .find(|descriptor| &descriptor.reference() == reference)
            .ok_or_else(|| {
                crate::error::provider_error(
                    ProviderErrorCategory::ModelNotFound,
                    self.id(),
                    Some(reference.model.clone()),
                    format!(
                        "model `{}` not found in Anthropic provider",
                        reference.model
                    ),
                )
            })?;

        Ok(Arc::new(
            AnthropicModel::new(self.args.clone(), authentication, descriptor).await?,
        ))
    }

    async fn list_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelReference>> {
        self.fetch_models(authentication)
            .await
            .map(|models| models.iter().map(ModelDescriptor::reference).collect())
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;
    use smista_core::model::Capability;

    use super::client::MockAnthropicClient;
    use super::client::api::{Capabilities, Model as ApiModel, Supported};
    use super::*;
    use crate::memory::MemoryRecord;

    /// Memory backend that stores nothing; resolution paths under test never
    /// touch it.
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

    fn api_model(id: &str, image: bool, structured: bool, thinking: bool) -> ApiModel {
        ApiModel {
            id: id.to_string(),
            display_name: Some(id.to_string()),
            max_input_tokens: 1_000_000,
            max_tokens: Some(128_000),
            capabilities: Capabilities {
                image_input: Supported { supported: image },
                structured_outputs: Supported {
                    supported: structured,
                },
                thinking: Supported {
                    supported: thinking,
                },
            },
        }
    }

    fn provider_with(models: Vec<ApiModel>) -> AnthropicProvider<MockAnthropicClient, NoStorage> {
        let client = MockAnthropicClient::new(Ok(models));
        AnthropicProvider::with_client(
            client,
            AnthropicModelArgs {
                preamble: "be helpful".to_string(),
                storage: Arc::new(NoStorage),
            },
        )
    }

    fn authentication() -> Authentication {
        Authentication::ApiKey(SecretString::from("sk-ant-test"))
    }

    #[test]
    fn should_identify_as_anthropic() {
        assert_eq!(provider_with(vec![]).id(), ProviderId::Anthropic);
    }

    #[test]
    fn should_seed_universal_capabilities_and_derive_the_rest() {
        let descriptor = AnthropicProvider::<MockAnthropicClient, NoStorage>::normalize_model(
            api_model("claude-opus-4-8", true, true, true),
        );

        // Universal Anthropic features are always on.
        assert!(descriptor.capabilities.supports(Capability::Streaming));
        assert!(descriptor.capabilities.supports(Capability::SystemPrompt));
        assert!(descriptor.capabilities.supports(Capability::Tools));
        assert!(descriptor.capabilities.supports(Capability::Memory));
        // Derived from the entry.
        assert!(descriptor.capabilities.supports(Capability::Images));
        assert!(descriptor.capabilities.supports(Capability::JsonOutput));
        assert!(descriptor.capabilities.supports(Capability::Reasoning));
        // Context sizes and price come through.
        assert_eq!(descriptor.max_context_tokens, 1_000_000);
        assert_eq!(descriptor.max_output_tokens, Some(128_000));
        assert_eq!(
            descriptor.input_cost_per_million_tokens,
            Some(rust_decimal::Decimal::new(5, 0))
        );
    }

    #[test]
    fn should_not_set_derived_capabilities_when_unreported() {
        let descriptor = AnthropicProvider::<MockAnthropicClient, NoStorage>::normalize_model(
            api_model("claude-opus-4-8", false, false, false),
        );

        assert!(!descriptor.capabilities.supports(Capability::Images));
        assert!(!descriptor.capabilities.supports(Capability::JsonOutput));
        assert!(!descriptor.capabilities.supports(Capability::Reasoning));
    }

    #[tokio::test]
    async fn should_list_models_from_the_api() {
        let provider = provider_with(vec![
            api_model("claude-opus-4-8", true, true, true),
            api_model("claude-haiku-4-5-20251001", true, true, true),
        ]);

        let listed = provider
            .list_models(&authentication())
            .await
            .expect("listing cannot fail");

        let ids: Vec<&str> = listed.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(ids, vec!["claude-opus-4-8", "claude-haiku-4-5-20251001"]);
    }

    #[tokio::test]
    async fn should_serve_repeat_lookups_from_the_cache() {
        let provider = provider_with(vec![api_model("claude-opus-4-8", true, true, true)]);

        provider
            .list_models(&authentication())
            .await
            .expect("first listing");
        provider
            .list_models(&authentication())
            .await
            .expect("second listing");

        // The API is queried once; the hot cache serves the second lookup.
        assert_eq!(provider.client.models_calls(), 1);
    }

    #[tokio::test]
    async fn should_reject_unknown_model_with_model_not_found() {
        let provider = provider_with(vec![api_model("claude-opus-4-8", true, true, true)]);

        let unknown = ModelReference {
            provider: ProviderId::Anthropic,
            model: "claude-does-not-exist".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider.resolve(&unknown, &authentication()).await else {
            panic!("an unoffered model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::Anthropic);
        assert_eq!(error.model.as_deref(), Some("claude-does-not-exist"));
    }

    #[tokio::test]
    async fn should_reject_resolution_without_an_api_key() {
        // An offered model still cannot be built without a credential: the model
        // build requires an API key, so a keyless authentication is rejected.
        let provider = provider_with(vec![api_model("claude-opus-4-8", true, true, true)]);
        let reference = ModelReference {
            provider: ProviderId::Anthropic,
            model: "claude-opus-4-8".to_string(),
        };

        let Err(error) = provider.resolve(&reference, &Authentication::None).await else {
            panic!("a model must not resolve without an API key");
        };

        assert_eq!(error.category, ProviderErrorCategory::MissingCredentials);
        assert_eq!(error.provider, ProviderId::Anthropic);
    }
}

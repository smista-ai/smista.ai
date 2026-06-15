//! Google Gemini [`Provider`] implementation.

pub mod client;

use std::sync::Arc;
use std::time::Duration;

use smista_core::error::ProviderErrorCategory;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    Provider as ProviderId, ProviderDescriptor,
};

use self::client::{GeminiClient, HttpGeminiClient};
use super::Provider;
use crate::ProviderResult;
use crate::auth::Authentication;
use crate::memory::{MemoryScope, MemoryStorage};
use crate::model::Model;
use crate::model::gemini::{self, GeminiModel, GeminiModelArgs};
use crate::provider::cache::{Cached, TtlCache};

/// How long the Gemini models cache stays fresh before the API is queried again.
///
/// The Gemini catalog changes rarely, so a long, fixed time-to-live keeps repeat
/// lookups off the API while bounding how stale the advertised list can be. One
/// hour.
const MODELS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// The generation method a model must support to be treated as a chat model.
///
/// The Gemini listing returns embedding, image, audio and other specialised
/// models alongside chat models; only those that can `generateContent` are
/// routed to.
const CHAT_GENERATION_METHOD: &str = "generateContent";

/// Substrings in a model id that mark a non-chat, specialised model.
///
/// Some image and speech models also advertise `generateContent` yet emit a
/// non-text modality smista.ai does not route to, so they are filtered out by id
/// in addition to the [`CHAT_GENERATION_METHOD`] check.
const NON_CHAT_MARKERS: &[&str] = &[
    "embedding",
    "image",
    "tts",
    "audio",
    "video",
    "imagen",
    "veo",
    "aqa",
];

/// A [`Provider`] backed by a single Google Gemini account.
///
/// Offers the Gemini chat models the account can see (fetched from
/// `v1beta/models`, filtered to chat models, and cached) and resolves a
/// [`ModelReference`] into an executable [`Model`] built from the shared
/// [`GeminiModelArgs`]. The credential is not held by the provider: it is
/// supplied per request as an [`Authentication`] when listing or resolving, so
/// one long-lived provider can serve many callers and owns its own model cache.
pub struct GeminiProvider<C, S>
where
    C: GeminiClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Client used to query the Gemini management endpoints.
    client: C,
    /// Shared arguments used to construct each resolved [`GeminiModel`].
    args: GeminiModelArgs<S>,
    /// Available models cache, refreshed with TTL.
    models: TtlCache<Vec<ModelDescriptor>>,
}

impl<S> GeminiProvider<HttpGeminiClient, S>
where
    S: MemoryStorage + 'static,
{
    /// Creates a new [`GeminiProvider`] targeting the public Gemini API.
    ///
    /// The provider owns its own models cache, created with a fixed TTL, so a
    /// single long-lived instance keeps repeat lookups off the API.
    pub fn new(args: GeminiModelArgs<S>) -> Self {
        Self::with_client(HttpGeminiClient::new(), args)
    }
}

impl<S> From<GeminiModelArgs<S>> for GeminiProvider<HttpGeminiClient, S>
where
    S: MemoryStorage + 'static,
{
    fn from(args: GeminiModelArgs<S>) -> Self {
        Self::new(args)
    }
}

impl<C, S> GeminiProvider<C, S>
where
    C: GeminiClient + 'static,
    S: MemoryStorage + 'static,
{
    /// Constructs a provider with an explicitly supplied management client.
    ///
    /// This is a testing and advanced seam: production code should use
    /// [`GeminiProvider::new`]. The provider owns a fresh models cache.
    pub fn with_client(client: C, args: GeminiModelArgs<S>) -> Self {
        Self {
            client,
            args,
            models: TtlCache::new(MODELS_CACHE_TTL),
        }
    }

    /// Fetches the available chat models, using the cache when it is still fresh.
    async fn fetch_models(
        &self,
        authentication: &Authentication,
    ) -> ProviderResult<Vec<ModelDescriptor>> {
        if let Cached::Hit(cached) = self.models.get() {
            tracing::debug!("Gemini models cache hit: {} model(s)", cached.len());
            return Ok(cached);
        }

        tracing::debug!("Gemini models cache miss, fetching from API");
        let descriptors: Vec<ModelDescriptor> = self
            .client
            .models(authentication)
            .await?
            .into_iter()
            .filter(Self::is_chat_model)
            .map(Self::normalize_model)
            .collect();

        tracing::debug!(
            "Fetched {} chat model(s) from Gemini API",
            descriptors.len()
        );
        self.models.set(descriptors.clone());

        Ok(descriptors)
    }

    /// Whether an API model entry is a chat model smista.ai routes to.
    ///
    /// Keeps entries that advertise the [`CHAT_GENERATION_METHOD`] and whose id
    /// carries none of the [`NON_CHAT_MARKERS`], dropping the embedding, image,
    /// audio and other specialised models the listing also returns.
    fn is_chat_model(model: &client::api::Model) -> bool {
        let supports_chat = model
            .supported_generation_methods
            .iter()
            .any(|method| method == CHAT_GENERATION_METHOD);

        let id = model.id().to_ascii_lowercase();
        let specialised = NON_CHAT_MARKERS.iter().any(|marker| id.contains(marker));

        supports_chat && !specialised
    }

    /// Turns an API model entry into a [`ModelDescriptor`].
    ///
    /// Streaming, system-prompt, tools, memory, JSON output and image input are
    /// universal Gemini chat features the listing does not flag per model, so
    /// they are seeded on; reasoning comes from the entry's thinking flag, the
    /// context sizes come from the entry, and the price comes from the table.
    fn normalize_model(model: client::api::Model) -> ModelDescriptor {
        let capabilities = ModelCapabilities {
            streaming: true,
            system_prompt: true,
            tools: true,
            memory: true,
            json_output: true,
            images: true,
            reasoning: model.thinking,
        };

        let id = model.id().to_string();
        let (input_cost, output_cost) = gemini::pricing(&id);

        ModelDescriptor {
            provider: ProviderId::Gemini,
            model: id,
            display_name: model.display_name,
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities,
            max_context_tokens: model.input_token_limit,
            max_output_tokens: model.output_token_limit,
            input_cost_per_million_tokens: input_cost,
            output_cost_per_million_tokens: output_cost,
            default_parameters: ModelParameters::default(),
            provider_options: None,
        }
    }
}

#[async_trait::async_trait]
impl<C, S> Provider for GeminiProvider<C, S>
where
    C: GeminiClient + 'static,
    S: MemoryStorage + 'static,
{
    fn id(&self) -> ProviderId {
        ProviderId::Gemini
    }

    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: ProviderId::Gemini,
            display_name: "Gemini".to_string(),
            local: false,
        }
    }

    async fn resolve(
        &self,
        reference: &ModelReference,
        authentication: &Authentication,
        scope: MemoryScope,
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
                    format!("model `{}` not found in Gemini provider", reference.model),
                )
            })?;

        Ok(Arc::new(
            GeminiModel::new(self.args.clone(), authentication, descriptor, scope).await?,
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
    use uuid::Uuid;

    use super::client::MockGeminiClient;
    use super::client::api::Model as ApiModel;
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

    /// Builds an API model entry with the given id, generation methods and
    /// thinking flag, naming it with the `models/` prefix the listing uses.
    fn api_model(id: &str, methods: &[&str], thinking: bool) -> ApiModel {
        ApiModel {
            name: format!("models/{id}"),
            display_name: Some(id.to_string()),
            description: None,
            input_token_limit: 1_048_576,
            output_token_limit: Some(65_536),
            supported_generation_methods: methods.iter().map(|m| m.to_string()).collect(),
            thinking,
        }
    }

    /// Shorthand for a chat model entry: it supports `generateContent`.
    fn chat_model(id: &str, thinking: bool) -> ApiModel {
        api_model(id, &["generateContent", "countTokens"], thinking)
    }

    fn provider_with(models: Vec<ApiModel>) -> GeminiProvider<MockGeminiClient, NoStorage> {
        let client = MockGeminiClient::new(Ok(models));
        GeminiProvider::with_client(
            client,
            GeminiModelArgs {
                preamble: "be helpful".to_string(),
                storage: Arc::new(NoStorage),
            },
        )
    }

    fn authentication() -> Authentication {
        Authentication::ApiKey(SecretString::from("gemini-test"))
    }

    #[test]
    fn should_identify_as_gemini() {
        assert_eq!(provider_with(vec![]).id(), ProviderId::Gemini);
    }

    #[test]
    fn should_keep_chat_models_and_drop_specialised_ones() {
        // Chat: supports generateContent, no specialised marker.
        assert!(
            GeminiProvider::<MockGeminiClient, NoStorage>::is_chat_model(&chat_model(
                "gemini-2.5-flash",
                true
            ))
        );
        // Embedding: no generateContent method.
        assert!(
            !GeminiProvider::<MockGeminiClient, NoStorage>::is_chat_model(&api_model(
                "gemini-embedding-001",
                &["embedContent"],
                false
            ))
        );
        // Image and speech variants advertise generateContent but emit a
        // non-text modality, so the id markers filter them out.
        assert!(
            !GeminiProvider::<MockGeminiClient, NoStorage>::is_chat_model(&chat_model(
                "gemini-2.5-flash-image",
                false
            ))
        );
        assert!(
            !GeminiProvider::<MockGeminiClient, NoStorage>::is_chat_model(&chat_model(
                "gemini-2.5-flash-preview-tts",
                false
            ))
        );
    }

    #[test]
    fn should_seed_universal_capabilities_and_derive_reasoning() {
        let descriptor = GeminiProvider::<MockGeminiClient, NoStorage>::normalize_model(
            chat_model("gemini-2.5-pro", true),
        );

        // Universal Gemini chat features are always on.
        assert!(descriptor.capabilities.supports(Capability::Streaming));
        assert!(descriptor.capabilities.supports(Capability::SystemPrompt));
        assert!(descriptor.capabilities.supports(Capability::Tools));
        assert!(descriptor.capabilities.supports(Capability::Memory));
        assert!(descriptor.capabilities.supports(Capability::JsonOutput));
        assert!(descriptor.capabilities.supports(Capability::Images));
        // Reasoning is derived from the thinking flag.
        assert!(descriptor.capabilities.supports(Capability::Reasoning));
        // The `models/` prefix is stripped, sizes and price come through.
        assert_eq!(descriptor.model, "gemini-2.5-pro");
        assert_eq!(descriptor.max_context_tokens, 1_048_576);
        assert_eq!(descriptor.max_output_tokens, Some(65_536));
        assert_eq!(
            descriptor.input_cost_per_million_tokens,
            Some(rust_decimal::Decimal::new(125, 2))
        );
    }

    #[test]
    fn should_not_set_reasoning_when_thinking_is_unreported() {
        let descriptor = GeminiProvider::<MockGeminiClient, NoStorage>::normalize_model(
            chat_model("gemini-2.5-flash", false),
        );

        assert!(!descriptor.capabilities.supports(Capability::Reasoning));
    }

    #[tokio::test]
    async fn should_list_only_chat_models_from_the_api() {
        let provider = provider_with(vec![
            chat_model("gemini-2.5-pro", true),
            chat_model("gemini-2.5-flash", true),
            api_model("text-embedding-004", &["embedContent"], false),
            chat_model("gemini-2.5-flash-image", false),
        ]);

        let listed = provider
            .list_models(&authentication())
            .await
            .expect("listing cannot fail");

        let ids: Vec<&str> = listed.iter().map(|r| r.model.as_str()).collect();
        assert_eq!(ids, vec!["gemini-2.5-pro", "gemini-2.5-flash"]);
    }

    #[tokio::test]
    async fn should_serve_repeat_lookups_from_the_cache() {
        let provider = provider_with(vec![chat_model("gemini-2.5-flash", true)]);

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
        let provider = provider_with(vec![chat_model("gemini-2.5-flash", true)]);

        let unknown = ModelReference {
            provider: ProviderId::Gemini,
            model: "gemini-2.0-flash".to_string(),
        };

        // `Arc<dyn Model>` is not `Debug`, so match rather than `expect_err`.
        let Err(error) = provider.resolve(&unknown, &authentication(), scope()).await else {
            panic!("an unoffered model must not resolve");
        };

        assert_eq!(error.category, ProviderErrorCategory::ModelNotFound);
        assert_eq!(error.provider, ProviderId::Gemini);
        assert_eq!(error.model.as_deref(), Some("gemini-2.0-flash"));
    }

    #[tokio::test]
    async fn should_reject_resolution_without_an_api_key() {
        // An offered model still cannot be built without a credential: the model
        // build requires an API key, so a keyless authentication is rejected.
        let provider = provider_with(vec![chat_model("gemini-2.5-flash", true)]);
        let reference = ModelReference {
            provider: ProviderId::Gemini,
            model: "gemini-2.5-flash".to_string(),
        };

        let Err(error) = provider
            .resolve(&reference, &Authentication::None, scope())
            .await
        else {
            panic!("a model must not resolve without an API key");
        };

        assert_eq!(error.category, ProviderErrorCategory::MissingCredentials);
        assert_eq!(error.provider, ProviderId::Gemini);
    }
}

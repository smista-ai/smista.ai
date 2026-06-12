//! Gemini Flash model implementations for the Gemini provider.

use rig_core::providers::gemini::client::Client as GeminiClient;
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    Provider,
};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::auth::Authentication;
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::gemini::GeminiModelArgs;

// `rig_core` exposes a `GEMINI_2_5_FLASH` constant but none for the 3.5 id, so
// both names are spelled out here for consistency, matching the Gemini API.
const GEMINI_2_5_FLASH: &str = "gemini-2.5-flash";
const GEMINI_3_5_FLASH: &str = "gemini-3.5-flash";

/// Returns the [`ModelReference`] for the Gemini 2.5 Flash model.
pub fn gemini_2_5_flash() -> ModelReference {
    ModelReference {
        provider: Provider::Gemini,
        model: GEMINI_2_5_FLASH.to_string(),
    }
}

/// Returns the [`ModelReference`] for the Gemini 3.5 Flash model.
pub fn gemini_3_5_flash() -> ModelReference {
    ModelReference {
        provider: Provider::Gemini,
        model: GEMINI_3_5_FLASH.to_string(),
    }
}

/// Gemini 2.5 Flash model
#[allow(non_camel_case_types)]
pub struct Gemini_2_5_Flash {
    agent: Agent<GeminiClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Gemini_2_5_Flash {
    /// Creates a new Gemini 2.5 Flash model with the given arguments,
    /// authenticating with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        GeminiModelArgs { preamble, storage }: GeminiModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Gemini {GEMINI_2_5_FLASH} model");
        let api_key = authentication.require_api_key(Provider::Gemini, GEMINI_2_5_FLASH)?;
        let client = GeminiClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Gemini,
                Some(GEMINI_2_5_FLASH.to_string()),
                "Failed to create Gemini client",
            )
        })?;

        let descriptor = ModelDescriptor {
            provider: Provider::Gemini,
            model: GEMINI_2_5_FLASH.to_string(),
            display_name: Some("Gemini 2.5 Flash".to_string()),
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            // Gemini 2.5 Flash is a multimodal thinking model: it accepts image
            // input, calls tools, emits structured output, and reasons.
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                json_output: true,
                system_prompt: true,
                images: true,
                reasoning: true,
                memory: true,
            },
            max_context_tokens: 1_048_576,
            max_output_tokens: Some(65_536),
            // Default Standard text tier. Gemini also has modality-specific
            // input pricing, a higher tier above the 200k-token prompt
            // breakpoint, and batch/flex/priority tiers that the descriptor
            // does not model; only the default text rate is stored here.
            input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(30, 2)), // $0.30 per million input tokens
            output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(250, 2)), // $2.50 per million output tokens
            default_parameters: ModelParameters::default(),
            provider_options: None,
        };

        tracing::debug!("Creating agent for Gemini {GEMINI_2_5_FLASH} model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Gemini {GEMINI_2_5_FLASH} model");
        Ok(Self {
            agent,
            descriptor,
            reference: gemini_2_5_flash(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Gemini_2_5_Flash {
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

/// Gemini 3.5 Flash model
#[allow(non_camel_case_types)]
pub struct Gemini_3_5_Flash {
    agent: Agent<GeminiClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Gemini_3_5_Flash {
    /// Creates a new Gemini 3.5 Flash model with the given arguments,
    /// authenticating with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        GeminiModelArgs { preamble, storage }: GeminiModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Gemini {GEMINI_3_5_FLASH} model");
        let api_key = authentication.require_api_key(Provider::Gemini, GEMINI_3_5_FLASH)?;
        let client = GeminiClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Gemini,
                Some(GEMINI_3_5_FLASH.to_string()),
                "Failed to create Gemini client",
            )
        })?;

        let descriptor = ModelDescriptor {
            provider: Provider::Gemini,
            model: GEMINI_3_5_FLASH.to_string(),
            display_name: Some("Gemini 3.5 Flash".to_string()),
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            // Gemini 3.5 Flash is a multimodal thinking model: it accepts image
            // input, calls tools, emits structured output, and reasons.
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                json_output: true,
                system_prompt: true,
                images: true,
                reasoning: true,
                memory: true,
            },
            max_context_tokens: 1_048_576,
            max_output_tokens: Some(65_536),
            // Default Standard text tier; see the note on `Gemini_2_5_Flash` for
            // the pricing dimensions the descriptor intentionally omits.
            input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(150, 2)), // $1.50 per million input tokens
            output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(9, 0)), // $9.00 per million output tokens
            default_parameters: ModelParameters::default(),
            provider_options: None,
        };

        tracing::debug!("Creating agent for Gemini {GEMINI_3_5_FLASH} model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Gemini {GEMINI_3_5_FLASH} model");
        Ok(Self {
            agent,
            descriptor,
            reference: gemini_3_5_flash(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Gemini_3_5_Flash {
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

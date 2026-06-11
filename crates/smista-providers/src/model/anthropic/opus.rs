//! Claude Opus models

use rig_core::providers::anthropic::client::Client as AnthropicClient;
use rig_core::providers::anthropic::completion::{
    CLAUDE_OPUS_4_6, CLAUDE_OPUS_4_7, CLAUDE_OPUS_4_8,
};
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
use crate::model::anthropic::AnthropicModelArgs;

/// Returns the [`ModelReference`] for the Opus 4.6 model.
pub fn opus_4_6() -> ModelReference {
    ModelReference {
        provider: Provider::Anthropic,
        model: CLAUDE_OPUS_4_6.to_string(),
    }
}

/// Returns the [`ModelReference`] for the Opus 4.7 model.
pub fn opus_4_7() -> ModelReference {
    ModelReference {
        provider: Provider::Anthropic,
        model: CLAUDE_OPUS_4_7.to_string(),
    }
}

/// Returns the [`ModelReference`] for the Opus 4.8 model.
pub fn opus_4_8() -> ModelReference {
    ModelReference {
        provider: Provider::Anthropic,
        model: CLAUDE_OPUS_4_8.to_string(),
    }
}

/// Opus 4.8 model
#[allow(non_camel_case_types)]
pub struct Opus_4_8 {
    agent: Agent<AnthropicClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Opus_4_8 {
    /// Creates a new Opus 4.8 model with the given arguments, authenticating
    /// with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        AnthropicModelArgs { preamble, storage }: AnthropicModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Anthropic Opus 4.8 model");
        let api_key = authentication.require_api_key(Provider::Anthropic, CLAUDE_OPUS_4_8)?;
        let client = AnthropicClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Anthropic,
                Some(CLAUDE_OPUS_4_8.to_string()),
                "Failed to create Anthropic client",
            )
        })?;

        tracing::debug!("Creating agent for Anthropic Opus 4.8 model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: CLAUDE_OPUS_4_8.to_string(),
            preamble,
            provider: Provider::Anthropic,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Anthropic Opus 4.8 model");
        Ok(Self {
            agent,
            descriptor: ModelDescriptor {
                provider: Provider::Anthropic,
                model: CLAUDE_OPUS_4_8.to_string(),
                display_name: Some("Claude Opus 4.8".to_string()),
                local: false,
                auth: ModelAuthRequirement::ApiKey,
                capabilities: ModelCapabilities {
                    streaming: true,
                    tools: true,
                    json_output: true,
                    system_prompt: true,
                    images: true,
                    reasoning: true,
                    memory: true,
                },
                max_context_tokens: 1_000_000,
                max_output_tokens: Some(128_000),
                input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(5, 0)), // $5.00 per million input tokens
                output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(25, 0)), // $25.00 per million output tokens
                // `temperature` / `top_p` are removed on Opus 4.8 (400 if sent),
                // so the default parameters stay empty.
                default_parameters: ModelParameters::default(),
                provider_options: None,
            },
            reference: opus_4_8(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Opus_4_8 {
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

/// Opus 4.7 model
#[allow(non_camel_case_types)]
pub struct Opus_4_7 {
    agent: Agent<AnthropicClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Opus_4_7 {
    /// Creates a new Opus 4.7 model with the given arguments, authenticating
    /// with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        AnthropicModelArgs { preamble, storage }: AnthropicModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Anthropic Opus 4.7 model");
        let api_key = authentication.require_api_key(Provider::Anthropic, CLAUDE_OPUS_4_7)?;
        let client = AnthropicClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Anthropic,
                Some(CLAUDE_OPUS_4_7.to_string()),
                "Failed to create Anthropic client",
            )
        })?;

        tracing::debug!("Creating agent for Anthropic Opus 4.7 model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: CLAUDE_OPUS_4_7.to_string(),
            preamble,
            provider: Provider::Anthropic,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Anthropic Opus 4.7 model");
        Ok(Self {
            agent,
            descriptor: ModelDescriptor {
                provider: Provider::Anthropic,
                model: CLAUDE_OPUS_4_7.to_string(),
                display_name: Some("Claude Opus 4.7".to_string()),
                local: false,
                auth: ModelAuthRequirement::ApiKey,
                capabilities: ModelCapabilities {
                    streaming: true,
                    tools: true,
                    // Native structured outputs are not supported on Opus 4.7.
                    json_output: false,
                    system_prompt: true,
                    images: true,
                    reasoning: true,
                    memory: true,
                },
                max_context_tokens: 1_000_000,
                max_output_tokens: Some(128_000),
                input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(5, 0)), // $5.00 per million input tokens
                output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(25, 0)), // $25.00 per million output tokens
                // `temperature` / `top_p` are removed on Opus 4.7 (400 if sent),
                // so the default parameters stay empty.
                default_parameters: ModelParameters::default(),
                provider_options: None,
            },
            reference: opus_4_7(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Opus_4_7 {
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

/// Opus 4.6 model
#[allow(non_camel_case_types)]
pub struct Opus_4_6 {
    agent: Agent<AnthropicClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Opus_4_6 {
    /// Creates a new Opus 4.6 model with the given arguments, authenticating
    /// with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        AnthropicModelArgs { preamble, storage }: AnthropicModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Anthropic Opus 4.6 model");
        let api_key = authentication.require_api_key(Provider::Anthropic, CLAUDE_OPUS_4_6)?;
        let client = AnthropicClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Anthropic,
                Some(CLAUDE_OPUS_4_6.to_string()),
                "Failed to create Anthropic client",
            )
        })?;

        tracing::debug!("Creating agent for Anthropic Opus 4.6 model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: CLAUDE_OPUS_4_6.to_string(),
            preamble,
            provider: Provider::Anthropic,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Anthropic Opus 4.6 model");
        Ok(Self {
            agent,
            descriptor: ModelDescriptor {
                provider: Provider::Anthropic,
                model: CLAUDE_OPUS_4_6.to_string(),
                display_name: Some("Claude Opus 4.6".to_string()),
                local: false,
                auth: ModelAuthRequirement::ApiKey,
                capabilities: ModelCapabilities {
                    streaming: true,
                    tools: true,
                    // Native structured outputs are not supported on Opus 4.6.
                    json_output: false,
                    system_prompt: true,
                    images: true,
                    reasoning: true,
                    memory: true,
                },
                max_context_tokens: 1_000_000,
                max_output_tokens: Some(128_000),
                input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(5, 0)), // $5.00 per million input tokens
                output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(25, 0)), // $25.00 per million output tokens
                default_parameters: ModelParameters::default(),
                provider_options: None,
            },
            reference: opus_4_6(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Opus_4_6 {
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

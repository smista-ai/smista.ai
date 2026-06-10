//! Claude Sonnet model

use rig_core::providers::anthropic::client::Client as AnthropicClient;
use rig_core::providers::anthropic::completion::CLAUDE_SONNET_4_6;
use secrecy::ExposeSecret;
use smista_core::error::ProviderError;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, ModelReference,
    Provider,
};

use crate::ProviderResult;
use crate::agent::{Agent, AgentArgs};
use crate::api::{CompletionRequest, CompletionResponse, ResponseStream};
use crate::memory::MemoryStorage;
use crate::model::Model;
use crate::model::anthropic::AnthropicModelArgs;

/// Returns a [`ModelReference`] for the Sonnet 4.6 model.
pub fn sonnet_4_6() -> ModelReference {
    ModelReference {
        provider: Provider::Anthropic,
        model: CLAUDE_SONNET_4_6.to_string(),
    }
}

/// Sonnet 4.6 model
#[allow(non_camel_case_types)]
pub struct Sonnet_4_6 {
    agent: Agent<AnthropicClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Sonnet_4_6 {
    /// Creates a new Sonnet 4.6 model with the given arguments.
    pub async fn new<S>(
        AnthropicModelArgs {
            apikey,
            preamble,
            storage,
        }: AnthropicModelArgs<S>,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Anthropic Sonnet 4.6 model");
        let client = AnthropicClient::new(apikey.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Anthropic,
                Some(CLAUDE_SONNET_4_6.to_string()),
                "Failed to create Anthropic client",
            )
        })?;

        tracing::debug!("Creating agent for Anthropic Sonnet 4.6 model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: CLAUDE_SONNET_4_6.to_string(),
            preamble,
            provider: Provider::Anthropic,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Anthropic Sonnet 4.6 model");
        Ok(Self {
            agent,
            descriptor: ModelDescriptor {
                provider: Provider::Anthropic,
                model: CLAUDE_SONNET_4_6.to_string(),
                display_name: Some("Claude Sonnet 4.6".to_string()),
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
                max_output_tokens: Some(64_000),
                input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(3, 0)), // $3.00 per million input tokens
                output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(15, 0)), // $15.00 per million output tokens
                default_parameters: ModelParameters::default(),
                provider_options: None,
            },
            reference: sonnet_4_6(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Sonnet_4_6 {
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

//! Claude Haiku model

use rig_core::providers::anthropic::client::Client as AnthropicClient;
use rig_core::providers::anthropic::completion::CLAUDE_HAIKU_4_5;
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

/// Returns the [`ModelReference`] for the Haiku 4.5 model.
pub fn haiku_4_5() -> ModelReference {
    ModelReference {
        provider: Provider::Anthropic,
        model: CLAUDE_HAIKU_4_5.to_string(),
    }
}

/// Haiku 4.5 model
#[allow(non_camel_case_types)]
pub struct Haiku_4_5 {
    agent: Agent<AnthropicClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Haiku_4_5 {
    /// Creates a new Haiku 4.5 model with the given arguments.
    pub async fn new<S>(
        AnthropicModelArgs {
            api_key,
            preamble,
            storage,
        }: AnthropicModelArgs<S>,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating Anthropic Haiku 4.5 model");
        let client = AnthropicClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::Anthropic,
                Some(CLAUDE_HAIKU_4_5.to_string()),
                "Failed to create Anthropic client",
            )
        })?;

        tracing::debug!("Creating agent for Anthropic Haiku 4.5 model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            model: CLAUDE_HAIKU_4_5.to_string(),
            preamble,
            provider: Provider::Anthropic,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created Anthropic Haiku 4.5 model");
        Ok(Self {
            agent,
            descriptor: ModelDescriptor {
                provider: Provider::Anthropic,
                model: CLAUDE_HAIKU_4_5.to_string(),
                display_name: Some("Claude Haiku 4.5".to_string()),
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
                max_context_tokens: 200_000,
                max_output_tokens: Some(64_000),
                input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(1, 0)), // $1.00 per million input tokens
                output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(5, 0)), // $5.00 per million output tokens
                default_parameters: ModelParameters::default(),
                provider_options: None,
            },
            reference: haiku_4_5(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Haiku_4_5 {
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

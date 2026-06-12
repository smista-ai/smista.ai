//! GPT Model implementations for the OpenAI provider.

use rig_core::providers::openai::client::Client as OpenAIClient;
use rig_core::providers::openai::completion::GPT_5_5;
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
use crate::model::openai::OpenAIModelArgs;

const GPT_5_4: &str = "gpt-5.4";
const GPT_5_4_MINI: &str = "gpt-5.4-mini";

/// Returns the [`ModelReference`] for the GPT-5.5 model.
pub fn gpt_5_5() -> ModelReference {
    ModelReference {
        provider: Provider::OpenAI,
        model: GPT_5_5.to_string(),
    }
}

/// Returns the [`ModelReference`] for the GPT-5.4 model.
pub fn gpt_5_4() -> ModelReference {
    ModelReference {
        provider: Provider::OpenAI,
        model: GPT_5_4.to_string(),
    }
}

/// Returns the [`ModelReference`] for the GPT-5.4-mini model.
pub fn gpt_5_4_mini() -> ModelReference {
    ModelReference {
        provider: Provider::OpenAI,
        model: GPT_5_4_MINI.to_string(),
    }
}

/// GPT-5.4-mini model
#[allow(non_camel_case_types)]
pub struct Gpt_5_4_Mini {
    agent: Agent<OpenAIClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Gpt_5_4_Mini {
    /// Creates a new GPT-5.4-mini model with the given arguments, authenticating
    /// with the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        OpenAIModelArgs { preamble, storage }: OpenAIModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating OpenAI {GPT_5_4_MINI} model");
        let api_key = authentication.require_api_key(Provider::OpenAI, GPT_5_4_MINI)?;
        let client = OpenAIClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::OpenAI,
                Some(GPT_5_4_MINI.to_string()),
                "Failed to create OpenAI client",
            )
        })?;

        let descriptor = ModelDescriptor {
            provider: Provider::OpenAI,
            model: GPT_5_4_MINI.to_string(),
            display_name: Some("GPT-5.4 mini".to_string()),
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
            max_context_tokens: 400_000,
            max_output_tokens: Some(128_000),
            input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(75, 2)), // $0.75 per million input tokens
            output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(45, 1)), // $4.50 per million output tokens
            // `temperature` / `top_p` are removed on GPT-5.4-mini (400 if
            // sent), so the default parameters stay empty.
            default_parameters: ModelParameters::default(),
            provider_options: None,
        };

        tracing::debug!("Creating agent for {GPT_5_4_MINI} model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created {GPT_5_4_MINI} model");
        Ok(Self {
            agent,
            descriptor,
            reference: gpt_5_4_mini(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Gpt_5_4_Mini {
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

/// GPT-5.4 model
#[allow(non_camel_case_types)]
pub struct Gpt_5_4 {
    agent: Agent<OpenAIClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Gpt_5_4 {
    /// Creates a new GPT 5.4 model with the given arguments, authenticating with
    /// the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        OpenAIModelArgs { preamble, storage }: OpenAIModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating OpenAI {GPT_5_4} model");
        let api_key = authentication.require_api_key(Provider::OpenAI, GPT_5_4)?;
        let client = OpenAIClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::OpenAI,
                Some(GPT_5_4.to_string()),
                "Failed to create OpenAI client",
            )
        })?;

        let descriptor = ModelDescriptor {
            provider: Provider::OpenAI,
            model: GPT_5_4.to_string(),
            display_name: Some("GPT-5.4".to_string()),
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                // Native structured outputs are not supported on GPT-5.4.
                json_output: true,
                system_prompt: true,
                images: true,
                reasoning: true,
                memory: true,
            },
            max_context_tokens: 1_000_000,
            max_output_tokens: Some(128_000),
            input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(25, 1)), // $2.50 per million input tokens
            output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(15, 0)), // $15.00 per million output tokens
            // `temperature` / `top_p` are removed on GPT-5.4 (400 if sent),
            // so the default parameters stay empty.
            default_parameters: ModelParameters::default(),
            provider_options: None,
        };

        tracing::debug!("Creating agent for OpenAI {GPT_5_4} model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created OpenAI {GPT_5_4} model");
        Ok(Self {
            agent,
            descriptor,
            reference: gpt_5_4(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Gpt_5_4 {
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

/// GPT 5.5 model
#[allow(non_camel_case_types)]
pub struct Gpt_5_5 {
    agent: Agent<OpenAIClient>,
    descriptor: ModelDescriptor,
    reference: ModelReference,
}

impl Gpt_5_5 {
    /// Creates a new GPT 5.5 model with the given arguments, authenticating with
    /// the supplied [`Authentication`].
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`] with category
    /// [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials)
    /// when `authentication` carries no API key, or if the underlying client
    /// cannot be built or the agent fails to load its memory preamble.
    pub async fn new<S>(
        OpenAIModelArgs { preamble, storage }: OpenAIModelArgs<S>,
        authentication: &Authentication,
    ) -> Result<Self, ProviderError>
    where
        S: MemoryStorage + 'static,
    {
        tracing::debug!("Creating {GPT_5_5} model");
        let api_key = authentication.require_api_key(Provider::OpenAI, GPT_5_5)?;
        let client = OpenAIClient::new(api_key.expose_secret()).map_err(|e| {
            crate::error::provider_error(
                crate::error::category_from_http(&e),
                Provider::OpenAI,
                Some(GPT_5_5.to_string()),
                "Failed to create OpenAI client",
            )
        })?;

        let descriptor = ModelDescriptor {
            provider: Provider::OpenAI,
            model: GPT_5_5.to_string(),
            display_name: Some("GPT-5.5".to_string()),
            local: false,
            auth: ModelAuthRequirement::ApiKey,
            capabilities: ModelCapabilities {
                streaming: true,
                tools: true,
                // Native structured outputs are not supported on GPT-5.5.
                json_output: true,
                system_prompt: true,
                images: true,
                reasoning: true,
                memory: true,
            },
            max_context_tokens: 1_000_000,
            max_output_tokens: Some(128_000),
            input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(5, 0)), // $5.00 per million input tokens
            output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(30, 0)), // $30.00 per million output tokens
            default_parameters: ModelParameters::default(),
            provider_options: None,
        };

        tracing::debug!("Creating agent for {GPT_5_5} model");
        let agent = Agent::new(AgentArgs {
            completion_model: client,
            descriptor: descriptor.clone(),
            preamble,
            storage,
        })
        .await?;

        tracing::debug!("Successfully created {GPT_5_5} model");
        Ok(Self {
            agent,
            descriptor,
            reference: gpt_5_5(),
        })
    }
}

#[async_trait::async_trait]
impl Model for Gpt_5_5 {
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

//! GPT model facts for the OpenAI provider.
//!
//! Each function returns the [`ModelDescriptor`] — the bundle of facts (name,
//! limits, prices, capabilities) — for one GPT model. The provider hands the
//! descriptor to the shared [`OpenAIModel`](super::OpenAIModel) constructor; the
//! behaviour lives there, so adding a model is a matter of writing one more
//! facts function here.

use rig_core::providers::openai::completion::GPT_5_5;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
};

const GPT_5_4: &str = "gpt-5.4";
const GPT_5_4_MINI: &str = "gpt-5.4-mini";

/// Returns the [`ModelDescriptor`] for the GPT-5.5 model.
pub fn gpt_5_5() -> ModelDescriptor {
    ModelDescriptor {
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
    }
}

/// Returns the [`ModelDescriptor`] for the GPT-5.4 model.
pub fn gpt_5_4() -> ModelDescriptor {
    ModelDescriptor {
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
        // `temperature` / `top_p` are removed on GPT-5.4 (400 if sent), so the
        // default parameters stay empty.
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

/// Returns the [`ModelDescriptor`] for the GPT-5.4-mini model.
pub fn gpt_5_4_mini() -> ModelDescriptor {
    ModelDescriptor {
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
        // `temperature` / `top_p` are removed on GPT-5.4-mini (400 if sent), so
        // the default parameters stay empty.
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

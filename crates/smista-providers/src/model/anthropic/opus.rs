//! Claude Opus model facts.

use rig_core::providers::anthropic::completion::{
    CLAUDE_OPUS_4_6, CLAUDE_OPUS_4_7, CLAUDE_OPUS_4_8,
};
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
};

/// Returns the [`ModelDescriptor`] for the Opus 4.8 model.
pub fn opus_4_8() -> ModelDescriptor {
    ModelDescriptor {
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
        // `temperature` / `top_p` are removed on Opus 4.8 (400 if sent), so the
        // default parameters stay empty.
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

/// Returns the [`ModelDescriptor`] for the Opus 4.7 model.
pub fn opus_4_7() -> ModelDescriptor {
    ModelDescriptor {
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
        // `temperature` / `top_p` are removed on Opus 4.7 (400 if sent), so the
        // default parameters stay empty.
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

/// Returns the [`ModelDescriptor`] for the Opus 4.6 model.
pub fn opus_4_6() -> ModelDescriptor {
    ModelDescriptor {
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
    }
}

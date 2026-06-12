//! Claude Sonnet model facts.

use rig_core::providers::anthropic::completion::CLAUDE_SONNET_4_6;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
};

/// Returns the [`ModelDescriptor`] for the Sonnet 4.6 model.
pub fn sonnet_4_6() -> ModelDescriptor {
    ModelDescriptor {
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
    }
}

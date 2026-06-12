//! Claude Haiku model facts.

use rig_core::providers::anthropic::completion::CLAUDE_HAIKU_4_5;
use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
};

/// Returns the [`ModelDescriptor`] for the Haiku 4.5 model.
pub fn haiku_4_5() -> ModelDescriptor {
    ModelDescriptor {
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
    }
}

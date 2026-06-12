//! Gemini Flash model facts for the Gemini provider.

use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
};

// `rig_core` exposes a `GEMINI_2_5_FLASH` constant but none for the 3.5 id, so
// both names are spelled out here for consistency, matching the Gemini API.
const GEMINI_2_5_FLASH: &str = "gemini-2.5-flash";
const GEMINI_3_5_FLASH: &str = "gemini-3.5-flash";

/// Returns the [`ModelDescriptor`] for the Gemini 2.5 Flash model.
pub fn gemini_2_5_flash() -> ModelDescriptor {
    ModelDescriptor {
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
        // Default Standard text tier. Gemini also has modality-specific input
        // pricing, a higher tier above the 200k-token prompt breakpoint, and
        // batch/flex/priority tiers that the descriptor does not model; only the
        // default text rate is stored here.
        input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(30, 2)), // $0.30 per million input tokens
        output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(250, 2)), // $2.50 per million output tokens
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

/// Returns the [`ModelDescriptor`] for the Gemini 3.5 Flash model.
pub fn gemini_3_5_flash() -> ModelDescriptor {
    ModelDescriptor {
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
        // Default Standard text tier; see the note on `gemini_2_5_flash` for the
        // pricing dimensions the descriptor intentionally omits.
        input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(150, 2)), // $1.50 per million input tokens
        output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(9, 0)), // $9.00 per million output tokens
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

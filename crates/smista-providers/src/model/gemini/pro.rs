//! Gemini Pro model facts for the Gemini provider.

use smista_core::model::{
    ModelAuthRequirement, ModelCapabilities, ModelDescriptor, ModelParameters, Provider,
};

// `rig_core` does not expose constants for these ids (it still ships retired
// 2.0 ids only), so the names are spelled out here, matching the Gemini API.
const GEMINI_2_5_PRO: &str = "gemini-2.5-pro";
const GEMINI_3_1_PRO_PREVIEW: &str = "gemini-3.1-pro-preview";

/// Returns the [`ModelDescriptor`] for the Gemini 2.5 Pro model.
pub fn gemini_2_5_pro() -> ModelDescriptor {
    ModelDescriptor {
        provider: Provider::Gemini,
        model: GEMINI_2_5_PRO.to_string(),
        display_name: Some("Gemini 2.5 Pro".to_string()),
        local: false,
        auth: ModelAuthRequirement::ApiKey,
        // Gemini 2.5 Pro is a multimodal thinking model: it accepts image
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
        input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(125, 2)), // $1.25 per million input tokens
        output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(10, 0)), // $10.00 per million output tokens
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

/// Returns the [`ModelDescriptor`] for the Gemini 3.1 Pro Preview model.
pub fn gemini_3_1_pro_preview() -> ModelDescriptor {
    ModelDescriptor {
        provider: Provider::Gemini,
        model: GEMINI_3_1_PRO_PREVIEW.to_string(),
        display_name: Some("Gemini 3.1 Pro Preview".to_string()),
        local: false,
        auth: ModelAuthRequirement::ApiKey,
        // Gemini 3.1 Pro is a multimodal thinking model: it accepts image
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
        // Default Standard text tier; see the note on `gemini_2_5_pro` for the
        // pricing dimensions the descriptor intentionally omits.
        input_cost_per_million_tokens: Some(rust_decimal::Decimal::new(2, 0)), // $2.00 per million input tokens
        output_cost_per_million_tokens: Some(rust_decimal::Decimal::new(12, 0)), // $12.00 per million output tokens
        default_parameters: ModelParameters::default(),
        provider_options: None,
    }
}

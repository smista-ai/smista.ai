//! Routing-rule capability requirement checks.

use smista_sdk::core::model::{Capability, ModelCapabilities, ModelReference};

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;
use crate::config::model::ModelConfig;

/// Capabilities a `[models]` table can declare. The remaining capabilities
/// (`system_prompt`, `images`) cannot be expressed in CLI configuration, so a
/// rule requiring them is not checked here.
const CHECKED_CAPABILITIES: [Capability; 5] = [
    Capability::Streaming,
    Capability::Tools,
    Capability::JsonOutput,
    Capability::Reasoning,
    Capability::Memory,
];

/// Checks each routing rule's `requires_capabilities` against the model it
/// routes to and every fallback.
///
/// Pushes one [`ValidationCode::UnsupportedCapability`] error per
/// (model, required-but-unsupported capability) pair. Models that do not
/// resolve in `[models]` are skipped here; the reference check reports those.
pub fn check_capabilities(config: &Config, report: &mut ValidationReport) {
    let mut visit = |required: &ModelCapabilities, reference: &ModelReference, location: &str| {
        let Some(model) = config.models.get(&reference.to_string()) else {
            // An unresolved model is already reported by the reference check.
            return;
        };

        for capability in CHECKED_CAPABILITIES {
            if required.supports(capability) && !model_supports(model, capability) {
                tracing::warn!(
                    capabilities.model = %reference,
                    capabilities.capability = %capability,
                    capabilities.location = %location,
                    "model {{capabilities.model}} lacks required capability {{capabilities.capability}}"
                );
                report.push(ValidationError {
                    code: ValidationCode::UnsupportedCapability,
                    severity: Severity::Error,
                    message: format!(
                        "model `{reference}` does not support `{capability}` required by this rule; declare `supports_{capability} = true` on the model or route to one that supports it"
                    ),
                    location: Some(location.to_string()),
                });
            }
        }
    };

    for (index, rule) in config.routing.rules.iter().enumerate() {
        let Some(required) = &rule.requires_capabilities else {
            continue;
        };
        visit(
            required,
            &rule.model,
            &format!("routing.rules[{index}].model"),
        );
        for (f, fallback) in rule.fallbacks.iter().enumerate() {
            visit(
                required,
                fallback,
                &format!("routing.rules[{index}].fallbacks[{f}]"),
            );
        }
    }
}

/// Returns whether `model` declares support for `capability`.
fn model_supports(model: &ModelConfig, capability: Capability) -> bool {
    match capability {
        Capability::Streaming => model.supports_streaming,
        Capability::Tools => model.supports_tools,
        Capability::JsonOutput => model.supports_json_output,
        Capability::Reasoning => model.supports_reasoning,
        Capability::Memory => model.supports_memory,
        // Not expressible in `[models]`; never reached via CHECKED_CAPABILITIES.
        Capability::SystemPrompt | Capability::Images => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    const MODELS: &str = r#"
        [models."ollama/llama3"]
        provider = "ollama"
        name = "llama3"
        supports_tools = false
        max_context_tokens = 8000

        [models."anthropic/claude-sonnet"]
        provider = "anthropic"
        name = "claude-sonnet"
        supports_tools = true
        max_context_tokens = 200000
    "#;

    #[test]
    fn should_flag_rule_requiring_unsupported_capability() {
        let config = parse(
            &format!(
                r#"
                {MODELS}
                [[routing.rules]]
                name = "tools on local"
                requires_capabilities = {{ tools = true }}
                model = "ollama/llama3"
                "#
            ),
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_capabilities(&config, &mut report);
        let error = report
            .errors()
            .iter()
            .find(|e| e.code == ValidationCode::UnsupportedCapability)
            .expect("unsupported capability error");
        assert_eq!(error.location.as_deref(), Some("routing.rules[0].model"));
    }

    #[test]
    fn should_flag_fallback_lacking_required_capability() {
        let config = parse(
            &format!(
                r#"
                {MODELS}
                [[routing.rules]]
                name = "tools with weak fallback"
                requires_capabilities = {{ tools = true }}
                model = "anthropic/claude-sonnet"
                fallbacks = ["ollama/llama3"]
                "#
            ),
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_capabilities(&config, &mut report);
        let error = report
            .errors()
            .iter()
            .find(|e| e.code == ValidationCode::UnsupportedCapability)
            .expect("unsupported capability error");
        assert_eq!(
            error.location.as_deref(),
            Some("routing.rules[0].fallbacks[0]")
        );
    }

    #[test]
    fn should_pass_when_model_supports_required_capability() {
        let config = parse(
            &format!(
                r#"
                {MODELS}
                [[routing.rules]]
                name = "tools on capable model"
                requires_capabilities = {{ tools = true }}
                model = "anthropic/claude-sonnet"
                "#
            ),
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_capabilities(&config, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_ignore_rules_without_capability_requirements() {
        let config = parse(
            &format!(
                r#"
                {MODELS}
                [[routing.rules]]
                name = "no requirement"
                model = "ollama/llama3"
                "#
            ),
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_capabilities(&config, &mut report);
        assert!(report.is_ok());
    }
}

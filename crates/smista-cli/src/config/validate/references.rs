//! Provider and model reference resolution checks.

use smista_sdk::core::model::ModelReference;

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;

/// Checks every model reference resolves to a configured provider and model.
///
/// Covers each routing rule `model`/`fallbacks`, the default route
/// `model`/`fallbacks`. Pushes one finding per unresolved reference.
pub fn check_references(config: &Config, report: &mut ValidationReport) {
    let mut visit = |reference: &ModelReference, location: String| {
        if !config.providers.contains_key(&reference.provider) {
            report.push(ValidationError {
                code: ValidationCode::UnknownProvider,
                severity: Severity::Error,
                message: format!(
                    "provider `{}` is not configured; add a [providers.{}] table",
                    reference.provider, reference.provider
                ),
                location: Some(location.clone()),
            });
        }
        if !config.models.contains_key(&reference.to_string()) {
            report.push(ValidationError {
                code: ValidationCode::UnknownModel,
                severity: Severity::Error,
                message: format!(
                    "model `{reference}` is not declared; add a [models.\"{reference}\"] table"
                ),
                location: Some(location),
            });
        }
    };

    for (index, rule) in config.routing.rules.iter().enumerate() {
        visit(&rule.model, format!("routing.rules[{index}].model"));
        for (f, fallback) in rule.fallbacks.iter().enumerate() {
            visit(fallback, format!("routing.rules[{index}].fallbacks[{f}]"));
        }
    }
    if let Some(default) = &config.routing.default {
        visit(&default.model, "routing.default.model".to_string());
        for (f, fallback) in default.fallbacks.iter().enumerate() {
            visit(fallback, format!("routing.default.fallbacks[{f}]"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_unknown_provider_and_model() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/gpt-5.5-mini"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_references(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::UnknownProvider)
        );
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::UnknownModel)
        );
    }

    #[test]
    fn should_pass_when_provider_and_model_declared() {
        let config = parse(
            r#"
            [providers.openai]
            type = "openai"

            [models."openai/gpt-5.5-mini"]
            provider = "openai"
            name = "gpt-5.5-mini"
            max_context_tokens = 128000

            [routing.default]
            model = "openai/gpt-5.5-mini"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_references(&config, &mut report);
        assert!(report.is_ok());
    }
}

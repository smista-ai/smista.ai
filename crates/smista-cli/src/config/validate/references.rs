//! Provider reference resolution checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;

/// Checks every model reference names a configured provider.
///
/// Model facts (and thus whether a `provider/model` exists) come from the
/// provider at runtime, not from config; that resolution happens at model
/// selection. The CLI can still verify the provider half, since `[providers]` is
/// user-authored. Covers each routing rule `model`/`fallbacks` and the default
/// route `model`/`fallbacks`. Pushes one finding per unresolved provider.
pub fn check_references(config: &Config, report: &mut ValidationReport) {
    tracing::trace!(
        references.rule_count = config.routing.rules.len(),
        references.provider_count = config.providers.len(),
        "checking provider references across {{references.rule_count}} rules"
    );
    let mut visit = |provider, location: String| {
        if !config.providers.contains_key(&provider) {
            tracing::warn!(
                references.provider = %provider,
                references.location = %location,
                "unresolved provider reference at {{references.location}}"
            );
            report.push(ValidationError {
                code: ValidationCode::UnknownProvider,
                severity: Severity::Error,
                message: format!(
                    "provider `{provider}` is not configured; add a [providers.{provider}] table"
                ),
                location: Some(location),
            });
        }
    };

    for (index, rule) in config.routing.rules.iter().enumerate() {
        visit(rule.model.provider, format!("routing.rules[{index}].model"));
        for (f, fallback) in rule.fallbacks.iter().enumerate() {
            visit(
                fallback.provider,
                format!("routing.rules[{index}].fallbacks[{f}]"),
            );
        }
    }
    if let Some(default) = &config.routing.default {
        visit(default.model.provider, "routing.default.model".to_string());
        for (f, fallback) in default.fallbacks.iter().enumerate() {
            visit(fallback.provider, format!("routing.default.fallbacks[{f}]"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_unknown_provider() {
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
    }

    #[test]
    fn should_pass_when_provider_declared() {
        let config = parse(
            r#"
            [providers.openai]
            type = "openai"

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

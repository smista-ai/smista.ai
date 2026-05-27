//! Inline-secret checks: credentials must be `${secret:..}` references.

use smista_core::secret::SecretRef;

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;

/// Flags any provider `api_key` that is an inline literal rather than a
/// `${secret:NAME}` reference. The offending value is never echoed.
pub fn check_inline_secrets(config: &Config, report: &mut ValidationReport) {
    for (provider, provider_config) in &config.providers {
        let Some(api_key) = &provider_config.api_key else {
            continue;
        };
        if SecretRef::parse(api_key).is_none() {
            report.push(ValidationError {
                code: ValidationCode::InlineSecret,
                severity: Severity::Error,
                message: format!(
                    "provider `{provider}` api_key is an inline literal; \
                     use a `${{secret:NAME}}` reference instead"
                ),
                location: Some(format!("providers.{provider}.api_key")),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_inline_literal_api_key() {
        let config = parse(
            r#"
            [providers.openai]
            type = "openai"
            api_key = "sk-literal-value"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_inline_secrets(&config, &mut report);
        let finding = report
            .errors()
            .iter()
            .find(|e| e.code == ValidationCode::InlineSecret)
            .expect("expected inline secret finding");
        assert!(!finding.message.contains("sk-literal-value"));
    }

    #[test]
    fn should_accept_secret_reference() {
        let config = parse(
            r#"
            [providers.openai]
            type = "openai"
            api_key = "${secret:OPENAI_API_KEY}"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_inline_secrets(&config, &mut report);
        assert!(report.is_ok());
    }
}

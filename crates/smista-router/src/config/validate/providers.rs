//! Provider configuration checks.

use smista_core::model::Provider;

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::RouterConfig;

/// A built-in provider talks to its fixed endpoint, so a `base_url` set on it is
/// ignored — flag it so the operator knows it has no effect.
///
/// The built-in API providers (Anthropic, Gemini, OpenAI) talk to fixed
/// endpoints. Ollama honors its `base_url` (the cloud host) and a generic
/// `openai-compat:<name>` endpoint genuinely needs one, so neither is flagged.
pub fn check_providers(config: &RouterConfig, report: &mut ValidationReport) {
    tracing::trace!("checking router provider configuration");
    for (provider_id, provider_config) in &config.providers {
        let ignores_base_url = matches!(
            provider_id,
            Provider::Anthropic | Provider::Gemini | Provider::OpenAI
        );
        if ignores_base_url && provider_config.base_url.is_some() {
            tracing::warn!(
                provider.id = %provider_id,
                "base_url set for built-in provider has no effect"
            );
            report.push(ValidationError {
                code: ValidationCode::IgnoredProviderBaseUrl,
                severity: Severity::Warning,
                message: format!(
                    "base_url is set for built-in provider `{provider_id}` but has no \
                     effect; use an openai-compat provider for a custom endpoint"
                ),
                location: Some(format!("router.providers.{provider_id}.base_url")),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_warn_when_built_in_provider_sets_base_url() {
        let config = parse(
            "[router.providers.openai]\nbase_url = \"https://proxy.internal/v1\"\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();

        check_providers(&config, &mut report);

        assert!(
            report
                .warnings()
                .iter()
                .any(|w| w.code == ValidationCode::IgnoredProviderBaseUrl),
            "expected a warning for a built-in provider's ignored base_url"
        );
        // A warning must not block startup.
        assert!(report.is_ok());
    }

    #[test]
    fn should_not_warn_when_built_in_provider_omits_base_url() {
        let config = parse("[router.providers.anthropic]\nlocal = false\n", "test").unwrap();
        let mut report = ValidationReport::default();

        check_providers(&config, &mut report);

        assert!(report.warnings().is_empty());
    }

    #[test]
    fn should_not_warn_for_ollama_base_url() {
        let config = parse(
            "[router.providers.ollama]\nbase_url = \"https://ollama.example.com\"\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();

        check_providers(&config, &mut report);

        assert!(
            report.warnings().is_empty(),
            "Ollama honors its base_url, so it must not be flagged"
        );
    }

    #[test]
    fn should_not_warn_for_openai_compatible_base_url() {
        let config = parse(
            "[router.providers.\"openai-compat:my-vllm\"]\nbase_url = \"http://localhost:8000/v1\"\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();

        check_providers(&config, &mut report);

        assert!(
            report.warnings().is_empty(),
            "openai-compat endpoints legitimately use base_url"
        );
    }
}

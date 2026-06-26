//! Validation of router runtime configuration.

mod auth;
mod binding;
mod cors;
mod limits;
mod opentelemetry;
mod providers;
mod rate_limit;
mod report;
mod storage;

pub use report::{Severity, ValidationCode, ValidationError, ValidationReport};

use crate::config::model::RouterConfig;

/// Validates router runtime configuration, collecting all findings.
///
/// Any error means the router must not start.
#[must_use]
pub fn validate(config: &RouterConfig) -> ValidationReport {
    tracing::debug!("validating router config");
    let mut report = ValidationReport::default();

    binding::check_binding(config, &mut report);
    storage::check_storage(config, &mut report);
    auth::check_auth(config, &mut report);
    limits::check_limits(config, &mut report);
    rate_limit::check_rate_limit(config, &mut report);
    cors::check_cors(config, &mut report);
    opentelemetry::check_opentelemetry(config, &mut report);
    providers::check_providers(config, &mut report);

    tracing::debug!(
        validation.errors = report.errors().len(),
        validation.warnings = report.warnings().len(),
        "router config validation complete: {{validation.errors}} errors, {{validation.warnings}} warnings"
    );
    if !report.is_ok() {
        tracing::warn!(
            validation.errors = report.errors().len(),
            "router config is invalid: {{validation.errors}} errors found"
        );
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_collect_multiple_errors_in_one_pass() {
        let config = parse(
            "[router]\nport = 0\n\n[router.storage]\nmode = \"remote\"\n",
            "test",
        )
        .unwrap();
        let report = validate(&config);
        assert!(!report.is_ok());
        assert!(report.errors().len() >= 2);
    }

    #[test]
    fn should_pass_complete_valid_fixture() {
        let config = parse(
            include_str!("../../tests/fixtures/router.toml"),
            "router.toml",
        )
        .unwrap();
        let report = validate(&config);
        assert!(
            report.is_ok(),
            "fixture should validate clean: {:?}",
            report.errors()
        );
    }
}

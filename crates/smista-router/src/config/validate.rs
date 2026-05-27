//! Validation of router runtime configuration.

mod auth;
mod binding;
mod cors;
mod limits;
mod report;
mod storage;

pub use report::{Severity, ValidationCode, ValidationError, ValidationReport};

use crate::config::model::RouterConfig;

/// Validates router runtime configuration, collecting all findings.
///
/// Any error means the router must not start.
#[must_use]
pub fn validate(config: &RouterConfig) -> ValidationReport {
    let mut report = ValidationReport::default();

    binding::check_binding(config, &mut report);
    storage::check_storage(config, &mut report);
    auth::check_auth(config, &mut report);
    limits::check_limits(config, &mut report);
    cors::check_cors(config, &mut report);

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

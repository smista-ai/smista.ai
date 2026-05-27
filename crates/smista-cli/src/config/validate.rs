//! Validation of merged CLI/policy configuration.

mod globs;
mod provenance;
mod references;
mod report;
mod routing;
mod secrets;

pub use report::{Severity, ValidationCode, ValidationError, ValidationReport};

use crate::config::Config;
use crate::config::layers::ConfigLayer;

/// Validates the merged configuration and its originating layer stack.
///
/// Collects every finding in one pass. Errors block; warnings are advisory.
#[must_use]
pub fn validate(merged: &Config, layers: &[(ConfigLayer, Config)]) -> ValidationReport {
    let mut report = ValidationReport::default();

    references::check_references(merged, &mut report);
    routing::check_routing_structure(merged, &mut report);
    routing::check_rule_ambiguity(merged, &mut report);
    globs::check_globs(merged, &mut report);
    secrets::check_inline_secrets(merged, &mut report);
    provenance::check_provenance(layers, &mut report);

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_pass_complete_valid_fixture() {
        let merged = parse(
            include_str!("../../tests/fixtures/config.toml"),
            "config.toml",
        )
        .unwrap();
        let layers = vec![(ConfigLayer::Project, merged.clone())];
        let report = validate(&merged, &layers);
        assert!(
            report.is_ok(),
            "fixture should validate clean: {:?}",
            report.errors()
        );
    }

    #[test]
    fn should_collect_errors_from_multiple_groups_in_one_pass() {
        // Missing default route (routing) + inline secret (secrets).
        let merged = parse(
            r#"
            [providers.openai]
            type = "openai"
            api_key = "literal"
            "#,
            "test",
        )
        .unwrap();
        let layers = vec![(ConfigLayer::Project, merged.clone())];
        let report = validate(&merged, &layers);
        assert!(!report.is_ok());
        assert!(report.errors().len() >= 2);
    }
}

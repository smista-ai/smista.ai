//! Validation of merged CLI/policy configuration.

mod globs;
mod provenance;
mod references;
mod report;
mod routing;
mod secrets;

pub use report::ValidationReport;

use crate::config::Config;
use crate::config::layers::ConfigLayer;

/// Validates the merged CLI configuration.
///
/// Collects every finding in one pass. Errors block configuration use.
#[must_use]
pub fn validate(config: &Config) -> ValidationReport {
    tracing::debug!("validating merged configuration");
    let mut report = ValidationReport::default();

    check_config(config, &mut report);

    tracing::debug!(
        validation.error_count = report.errors().len(),
        "validation complete"
    );
    report
}

/// Validates a merged configuration and its originating layer stack.
///
/// Use this when the caller still has access to the unmerged layers and wants
/// provenance checks for preference layers. Most callers should use
/// [`validate`], which mirrors the router config validator.
#[must_use]
pub fn validate_layers(merged: &Config, layers: &[(ConfigLayer, Config)]) -> ValidationReport {
    tracing::debug!(
        validation.layer_count = layers.len(),
        "validating merged configuration with layer provenance"
    );
    let mut report = validate(merged);

    tracing::trace!("checking layer provenance");
    provenance::check_provenance(layers, &mut report);

    tracing::debug!(
        validation.error_count = report.errors().len(),
        "validation complete"
    );
    report
}

/// Runs checks that only require the merged configuration.
fn check_config(config: &Config, report: &mut ValidationReport) {
    tracing::trace!("checking provider references");
    references::check_references(config, report);
    tracing::trace!("checking routing structure");
    routing::check_routing_structure(config, report);
    tracing::trace!("checking routing rule ambiguity");
    routing::check_rule_ambiguity(config, report);
    tracing::trace!("checking glob patterns");
    globs::check_globs(config, report);
    tracing::trace!("checking inline secrets");
    secrets::check_inline_secrets(config, report);
}

#[cfg(test)]
mod tests {
    use super::report::ValidationCode;
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_validate_minimal_authored_config_clean() {
        // A CLI config is user-authored: it must name a provider and a default
        // route. With only those present, every other field takes its default
        // and the merged configuration validates clean.
        let merged = parse(
            r#"
            [providers.openai]
            type = "openai"

            [routing.default]
            model = "openai/gpt-5.5-mini"
            "#,
            "test",
        )
        .unwrap();
        let report = validate(&merged);
        assert!(
            report.is_ok(),
            "minimal authored CLI config must validate clean: {:?}",
            report.errors()
        );
    }

    #[test]
    fn should_pass_complete_valid_fixture() {
        let merged = parse(
            include_str!("../../tests/fixtures/config.toml"),
            "config.toml",
        )
        .unwrap();
        let report = validate(&merged);
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
        let report = validate(&merged);
        assert!(!report.is_ok());
        assert!(report.errors().len() >= 2);
    }

    #[test]
    fn should_validate_layer_provenance_when_layers_are_available() {
        let project = parse(
            r#"
            [providers.openai]
            type = "openai"

            [routing.default]
            model = "openai/gpt-5.5-mini"

            [tools.permissions]
            shell = "deny"
            "#,
            "test",
        )
        .unwrap();
        let local = parse(
            r#"
            [tools.permissions]
            shell = "allow"
            "#,
            "test",
        )
        .unwrap();
        let merged = crate::config::layers::merge(vec![
            (ConfigLayer::Project, project.clone()),
            (ConfigLayer::RuntimeOverride, local.clone()),
        ]);
        let layers = vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::RuntimeOverride, local),
        ];

        let report = validate_layers(&merged, &layers);

        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::PermissionWidening)
        );
    }
}

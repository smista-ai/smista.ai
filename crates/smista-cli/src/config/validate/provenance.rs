//! Layer-provenance checks across config layers.
//!
//! The merge silently latches the stricter value, so weakening *attempts* are
//! only visible by comparing config layers to higher preference layers.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;
use crate::config::layers::{ConfigLayer, merge};

/// Runs all provenance checks over the ordered layer stack.
pub fn check_provenance(layers: &[(ConfigLayer, Config)], report: &mut ValidationReport) {
    tracing::trace!(
        provenance.layer_count = layers.len(),
        "checking layer provenance across layers"
    );
    let safety_floor = merge(
        layers
            .iter()
            .filter(|(layer, _)| !layer.is_preference())
            .cloned()
            .collect(),
    );
    let has_config_layer = layers
        .iter()
        .any(|(layer, config)| !layer.is_preference() && config != &Config::default());
    if !has_config_layer {
        tracing::trace!("no non-default config layer present; skipping provenance checks");
        return;
    }

    for (layer, config) in layers.iter().filter(|(layer, _)| layer.is_preference()) {
        tracing::trace!(provenance.layer = ?layer, "checking preference layer");
        check_unsafe_override(&safety_floor, *layer, config, report);
        check_permission_widening(&safety_floor, *layer, config, report);
    }
}

/// A preference layer must not weaken a stricter config-layer privacy mode.
fn check_unsafe_override(
    safety_floor: &Config,
    layer: ConfigLayer,
    config: &Config,
    report: &mut ValidationReport,
) {
    if let (Some(project_mode), Some(layer_mode)) =
        (safety_floor.privacy.remote.mode, config.privacy.remote.mode)
        && layer_mode < project_mode
    {
        tracing::warn!(
            provenance.layer = ?layer,
            provenance.floor_mode = ?project_mode,
            provenance.attempted_mode = ?layer_mode,
            provenance.field = "privacy.remote.mode",
            "preference layer weakens field"
        );
        report.push(ValidationError {
            code: ValidationCode::UnsafeOverride,
            severity: Severity::Error,
            message: format!(
                "{layer:?} layer weakens remote privacy mode from `{project_mode:?}` \
                 to `{layer_mode:?}`; this policy is not overridable"
            ),
            location: Some("privacy.remote.mode".into()),
        });
    }
}

/// A preference layer must not widen a tool permission beyond the config floor.
fn check_permission_widening(
    safety_floor: &Config,
    layer: ConfigLayer,
    config: &Config,
    report: &mut ValidationReport,
) {
    for (tool, layer_mode) in &config.tools.permissions {
        if let Some(project_mode) = safety_floor.tools.mode_for(tool)
            && *layer_mode < project_mode
        {
            tracing::warn!(
                provenance.layer = ?layer,
                provenance.tool = %tool,
                provenance.floor_mode = ?project_mode,
                provenance.attempted_mode = ?layer_mode,
                "preference layer widens permission for tool"
            );
            report.push(ValidationError {
                code: ValidationCode::PermissionWidening,
                severity: Severity::Error,
                message: format!(
                    "{layer:?} layer widens permission for `{tool}` from \
                     `{project_mode:?}` to `{layer_mode:?}`; not allowed"
                ),
                location: Some(format!("tools.permissions.{tool}")),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    fn layer(toml: &str) -> Config {
        parse(toml, "test").unwrap()
    }

    #[test]
    fn should_flag_permission_widening_by_preference_layer() {
        let project = layer(
            r#"
            [tools.permissions]
            shell = "deny"
            "#,
        );
        let local = layer(
            r#"
            [tools.permissions]
            shell = "allow"
            "#,
        );
        let stack = vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ];
        let mut report = ValidationReport::default();
        check_provenance(&stack, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::PermissionWidening)
        );
    }

    #[test]
    fn should_flag_permission_widening_against_global_config_layer() {
        let global = layer(
            r#"
            [tools.permissions]
            shell = "deny"
            "#,
        );
        let project = layer("");
        let local = layer(
            r#"
            [tools.permissions]
            shell = "allow"
            "#,
        );
        let stack = vec![
            (ConfigLayer::Global, global),
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ];
        let mut report = ValidationReport::default();
        check_provenance(&stack, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::PermissionWidening)
        );
    }

    #[test]
    fn should_flag_unsafe_privacy_override() {
        let project = layer(
            r#"
            [privacy.remote]
            mode = "deny"
            "#,
        );
        let runtime = layer(
            r#"
            [privacy.remote]
            mode = "allow"
            "#,
        );
        let stack = vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::RuntimeOverride, runtime),
        ];
        let mut report = ValidationReport::default();
        check_provenance(&stack, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::UnsafeOverride)
        );
    }

    #[test]
    fn should_flag_unsafe_privacy_override_against_global_config_layer() {
        let global = layer(
            r#"
            [privacy.remote]
            mode = "deny"
            "#,
        );
        let project = layer("");
        let runtime = layer(
            r#"
            [privacy.remote]
            mode = "allow"
            "#,
        );
        let stack = vec![
            (ConfigLayer::Global, global),
            (ConfigLayer::Project, project),
            (ConfigLayer::RuntimeOverride, runtime),
        ];
        let mut report = ValidationReport::default();
        check_provenance(&stack, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::UnsafeOverride)
        );
    }

    #[test]
    fn should_pass_when_preference_does_not_weaken() {
        let project = layer(
            r#"
            [tools.permissions]
            shell = "ask"
            "#,
        );
        let local = layer(
            r#"
            [tools.permissions]
            shell = "deny"
            "#,
        );
        let stack = vec![
            (ConfigLayer::Project, project),
            (ConfigLayer::LocalPrefs, local),
        ];
        let mut report = ValidationReport::default();
        check_provenance(&stack, &mut report);
        assert!(report.is_ok());
    }
}

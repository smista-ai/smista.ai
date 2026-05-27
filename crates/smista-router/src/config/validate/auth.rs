//! Authentication configuration checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::{RouterConfig, StorageMode};

/// Local API-key bootstrap must be disabled when storage runs in remote mode.
pub fn check_auth(config: &RouterConfig, report: &mut ValidationReport) {
    if config.storage.mode == StorageMode::Remote && config.auth.local_bootstrap_enabled {
        report.push(ValidationError {
            code: ValidationCode::LocalBootstrapInRemote,
            severity: Severity::Error,
            message: "local_bootstrap_enabled must be false when storage.mode = remote; \
                      disable it or switch to embedded mode"
                .into(),
            location: Some("router.auth.local_bootstrap_enabled".into()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_local_bootstrap_in_remote_mode() {
        let config = parse(
            "[router.storage]\nmode = \"remote\"\nurl = \"ws://db:8000\"\n\n[router.auth]\nlocal_bootstrap_enabled = true\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_auth(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::LocalBootstrapInRemote)
        );
    }

    #[test]
    fn should_accept_local_bootstrap_in_embedded_mode() {
        let config = parse("", "test").unwrap();
        let mut report = ValidationReport::default();
        check_auth(&config, &mut report);
        assert!(report.is_ok());
    }
}

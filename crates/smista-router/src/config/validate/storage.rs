//! Storage configuration checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::{RouterConfig, StorageMode};

/// Embedded mode requires a `path`; remote mode requires a `url`.
pub fn check_storage(config: &RouterConfig, report: &mut ValidationReport) {
    match config.storage.mode {
        StorageMode::Embedded if config.storage.path.is_none() => {
            report.push(ValidationError {
                code: ValidationCode::MissingStorageConfig,
                severity: Severity::Error,
                message: "embedded storage requires `storage.path`".into(),
                location: Some("router.storage.path".into()),
            });
        }
        StorageMode::Remote if config.storage.url.is_none() => {
            report.push(ValidationError {
                code: ValidationCode::MissingStorageConfig,
                severity: Severity::Error,
                message: "remote storage requires `storage.url`".into(),
                location: Some("router.storage.url".into()),
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_remote_without_url() {
        let config = parse("[router.storage]\nmode = \"remote\"\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_storage(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::MissingStorageConfig)
        );
    }

    #[test]
    fn should_accept_embedded_with_path() {
        let config = parse(
            "[router.storage]\nmode = \"embedded\"\npath = \".smista/db\"\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_storage(&config, &mut report);
        assert!(report.is_ok());
    }
}

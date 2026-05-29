//! CORS configuration checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::RouterConfig;

/// CORS enabled with `*` or no explicit origins is an unrestricted policy.
pub fn check_cors(config: &RouterConfig, report: &mut ValidationReport) {
    tracing::trace!("checking router CORS configuration");
    if !config.cors.enabled {
        tracing::debug!("CORS is disabled; skipping CORS checks");
        return;
    }
    let unrestricted = config.cors.allowed_origins.is_empty()
        || config
            .cors
            .allowed_origins
            .iter()
            .any(|origin| origin == "*");
    if unrestricted {
        tracing::warn!("CORS is enabled with unrestricted origins");
        report.push(ValidationError {
            code: ValidationCode::UnsafeCors,
            severity: Severity::Error,
            message: "CORS is enabled with unrestricted origins; \
                      list explicit origins in cors.allowed_origins"
                .into(),
            location: Some("router.cors.allowed_origins".into()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_wildcard_origin() {
        let config = parse(
            "[router.cors]\nenabled = true\nallowed_origins = [\"*\"]\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_cors(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::UnsafeCors)
        );
    }

    #[test]
    fn should_flag_enabled_without_origins() {
        let config = parse("[router.cors]\nenabled = true\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_cors(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::UnsafeCors)
        );
    }

    #[test]
    fn should_accept_explicit_origins() {
        let config = parse(
            "[router.cors]\nenabled = true\nallowed_origins = [\"https://app.example\"]\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_cors(&config, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_ignore_disabled_cors() {
        let config = parse("", "test").unwrap();
        let mut report = ValidationReport::default();
        check_cors(&config, &mut report);
        assert!(report.is_ok());
    }
}

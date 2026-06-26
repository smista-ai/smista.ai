//! OpenTelemetry configuration checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::RouterConfig;

/// Validates the OpenTelemetry export settings when export is enabled.
///
/// When disabled the settings are inert, so they are not checked. When enabled,
/// the collector endpoint and service name must be non-empty and the sampling
/// ratio must fall within `0.0..=1.0`, otherwise the exporter cannot be built.
pub fn check_opentelemetry(config: &RouterConfig, report: &mut ValidationReport) {
    tracing::trace!("checking router OpenTelemetry configuration");
    let otel = &config.opentelemetry;
    if !otel.enabled {
        tracing::debug!("OpenTelemetry is disabled; skipping OpenTelemetry checks");
        return;
    }

    if otel.endpoint.trim().is_empty() {
        tracing::warn!("OpenTelemetry is enabled with an empty collector endpoint");
        report.push(ValidationError {
            code: ValidationCode::InvalidOpenTelemetry,
            severity: Severity::Error,
            message: "OpenTelemetry is enabled but the collector endpoint is empty; \
                      set opentelemetry.endpoint to your collector address"
                .into(),
            location: Some("router.opentelemetry.endpoint".into()),
        });
    }

    if otel.service_name.trim().is_empty() {
        tracing::warn!("OpenTelemetry is enabled with an empty service name");
        report.push(ValidationError {
            code: ValidationCode::InvalidOpenTelemetry,
            severity: Severity::Error,
            message: "OpenTelemetry is enabled but the service name is empty; \
                      set opentelemetry.service_name"
                .into(),
            location: Some("router.opentelemetry.service_name".into()),
        });
    }

    if !(0.0..=1.0).contains(&otel.sample_ratio) {
        tracing::warn!(
            otel.sample_ratio = otel.sample_ratio,
            "OpenTelemetry sampling ratio is out of range"
        );
        report.push(ValidationError {
            code: ValidationCode::InvalidOpenTelemetry,
            severity: Severity::Error,
            message: "opentelemetry.sample_ratio must be between 0.0 and 1.0 inclusive".into(),
            location: Some("router.opentelemetry.sample_ratio".into()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_ignore_disabled_opentelemetry() {
        let config = parse(
            "[router.opentelemetry]\nenabled = false\nendpoint = \"\"\nsample_ratio = 5.0\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_opentelemetry(&config, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_flag_empty_endpoint() {
        let config = parse(
            "[router.opentelemetry]\nenabled = true\nendpoint = \"\"\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_opentelemetry(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidOpenTelemetry)
        );
    }

    #[test]
    fn should_flag_out_of_range_sample_ratio() {
        let config = parse(
            "[router.opentelemetry]\nenabled = true\nsample_ratio = 1.5\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_opentelemetry(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidOpenTelemetry)
        );
    }

    #[test]
    fn should_accept_valid_enabled_config() {
        let config = parse(
            "[router.opentelemetry]\nenabled = true\nendpoint = \"http://localhost:4317\"\nsample_ratio = 0.5\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_opentelemetry(&config, &mut report);
        assert!(report.is_ok());
    }
}

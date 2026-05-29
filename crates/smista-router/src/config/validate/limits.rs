//! Request/execution limit and timeout checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::RouterConfig;

/// One hour in milliseconds.
///
/// Timeouts above this threshold are almost certainly a unit misconfiguration
/// (e.g., seconds passed where milliseconds are expected). Chosen to allow
/// generous but realistic long-running tool or provider calls while still
/// catching obvious mistakes.
const MAX_SANE_TIMEOUT_MS: u64 = 3_600_000;

/// Validates limits: no zero timeout, no zero size limit, no absurd timeout.
pub fn check_limits(config: &RouterConfig, report: &mut ValidationReport) {
    tracing::trace!("checking router limits and timeouts");
    let timeouts = [
        (
            "router.limits.request_timeout_ms",
            config.limits.request_timeout_ms,
        ),
        (
            "router.limits.provider_timeout_ms",
            config.limits.provider_timeout_ms,
        ),
        (
            "router.limits.tool_timeout_ms",
            config.limits.tool_timeout_ms,
        ),
    ];
    for (field, value) in timeouts {
        if value == 0 {
            tracing::warn!(
                limits.field = %field,
                "timeout {{limits.field}} is zero"
            );
            report.push(ValidationError {
                code: ValidationCode::InvalidTimeout,
                severity: Severity::Error,
                message: format!("{field} must be greater than zero"),
                location: Some(field.into()),
            });
        } else if value > MAX_SANE_TIMEOUT_MS {
            tracing::warn!(
                limits.field = %field,
                limits.value = value,
                "timeout {{limits.field}} of {{limits.value}}ms is absurdly large"
            );
            report.push(ValidationError {
                code: ValidationCode::ExcessiveTimeout,
                severity: Severity::Warning,
                message: format!(
                    "{field} of {value}ms exceeds {MAX_SANE_TIMEOUT_MS}ms; check the unit"
                ),
                location: Some(field.into()),
            });
        }
    }

    let limits = [
        (
            "router.limits.max_request_body_bytes",
            config.limits.max_request_body_bytes,
        ),
        (
            "router.limits.max_context_bytes",
            config.limits.max_context_bytes,
        ),
    ];
    for (field, value) in limits {
        if value == 0 {
            tracing::warn!(
                limits.field = %field,
                "size limit {{limits.field}} is zero"
            );
            report.push(ValidationError {
                code: ValidationCode::InvalidLimit,
                severity: Severity::Error,
                message: format!("{field} must be greater than zero"),
                location: Some(field.into()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_zero_timeout() {
        let config = parse("[router.limits]\nrequest_timeout_ms = 0\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_limits(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidTimeout)
        );
    }

    #[test]
    fn should_warn_excessive_timeout() {
        let config = parse("[router.limits]\ntool_timeout_ms = 7200000\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_limits(&config, &mut report);
        assert!(
            report
                .warnings()
                .iter()
                .any(|e| e.code == ValidationCode::ExcessiveTimeout)
        );
        assert!(report.is_ok());
    }

    #[test]
    fn should_accept_default_limits() {
        let config = parse("", "test").unwrap();
        let mut report = ValidationReport::default();
        check_limits(&config, &mut report);
        assert!(report.is_ok());
        assert!(report.warnings().is_empty());
    }
}

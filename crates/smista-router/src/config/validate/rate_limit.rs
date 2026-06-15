//! HTTP rate-limiting configuration checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::RouterConfig;

/// Validates rate limiting: when enabled, neither the refill period nor the
/// burst size may be zero. A zero value collapses the token bucket and the
/// limiter cannot be constructed, so it is rejected before startup.
pub fn check_rate_limit(config: &RouterConfig, report: &mut ValidationReport) {
    tracing::trace!("checking router rate-limit configuration");
    let rate_limit = &config.rate_limit;
    if !rate_limit.enabled {
        return;
    }

    if rate_limit.period_ms == 0 {
        tracing::warn!("rate-limit period is zero while rate limiting is enabled");
        report.push(ValidationError {
            code: ValidationCode::InvalidRateLimit,
            severity: Severity::Error,
            message: "router.rate_limit.period_ms must be greater than zero when rate limiting is enabled".to_string(),
            location: Some("router.rate_limit.period_ms".into()),
        });
    }

    if rate_limit.burst_size == 0 {
        tracing::warn!("rate-limit burst size is zero while rate limiting is enabled");
        report.push(ValidationError {
            code: ValidationCode::InvalidRateLimit,
            severity: Severity::Error,
            message: "router.rate_limit.burst_size must be greater than zero when rate limiting is enabled".to_string(),
            location: Some("router.rate_limit.burst_size".into()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_zero_period_when_enabled() {
        let config = parse("[router.rate_limit]\nperiod_ms = 0\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_rate_limit(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidRateLimit)
        );
    }

    #[test]
    fn should_flag_zero_burst_when_enabled() {
        let config = parse("[router.rate_limit]\nburst_size = 0\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_rate_limit(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidRateLimit)
        );
    }

    #[test]
    fn should_ignore_zero_values_when_disabled() {
        let config = parse(
            "[router.rate_limit]\nenabled = false\nperiod_ms = 0\nburst_size = 0\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_rate_limit(&config, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_accept_default_rate_limit() {
        let config = parse("", "test").unwrap();
        let mut report = ValidationReport::default();
        check_rate_limit(&config, &mut report);
        assert!(report.is_ok());
    }
}

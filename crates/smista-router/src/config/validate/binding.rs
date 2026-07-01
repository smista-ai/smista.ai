//! Bind host/port checks.

use std::net::IpAddr;

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::model::{RouterConfig, StorageMode};

/// Public bind addresses that expose the router to all network interfaces.
const PUBLIC_HOSTS: [&str; 2] = ["0.0.0.0", "::"];

/// Validates the bind host and port, and warns on a public bind in embedded
/// mode (a local deployment exposed to the network).
pub fn check_binding(config: &RouterConfig, report: &mut ValidationReport) {
    tracing::trace!(
        bind.host = %config.host,
        bind.port = config.port,
        "checking router bind"
    );
    if config.port == 0 {
        tracing::warn!(bind.port = config.port, "bind port is invalid");
        report.push(ValidationError {
            code: ValidationCode::InvalidPort,
            severity: Severity::Error,
            message: "port 0 is not a valid bind port; set a port in 1-65535".into(),
            location: Some("router.port".into()),
        });
    }

    if !is_valid_host(&config.host) {
        tracing::warn!(bind.host = %config.host, "bind host is invalid");
        report.push(ValidationError {
            code: ValidationCode::InvalidHost,
            severity: Severity::Error,
            message: format!("bind host `{}` is not a valid host", config.host),
            location: Some("router.host".into()),
        });
    } else if PUBLIC_HOSTS.contains(&config.host.as_str())
        && config.storage.mode == StorageMode::Embedded
    {
        tracing::warn!(
            bind.host = %config.host,
            "public bind in embedded mode exposes a local router to the network"
        );
        report.push(ValidationError {
            code: ValidationCode::UnsafePublicBind,
            severity: Severity::Warning,
            message: format!(
                "binding `{}` in embedded mode exposes a local router to the network; \
                 bind 127.0.0.1 unless this is intentional",
                config.host
            ),
            location: Some("router.host".into()),
        });
    }
}

/// Returns whether `host` is a bare hostname or IP address without a port.
fn is_valid_host(host: &str) -> bool {
    if host.trim().is_empty() || host.contains(char::is_whitespace) {
        return false;
    }

    if host.contains(':') {
        return host.parse::<IpAddr>().is_ok();
    }

    host.parse::<IpAddr>().is_ok() || is_valid_dns_name(host)
}

/// Validates a conservative DNS hostname shape.
fn is_valid_dns_name(host: &str) -> bool {
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_port_zero() {
        let config = parse("[router]\nport = 0\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidPort)
        );
    }

    #[test]
    fn should_flag_host_with_embedded_port() {
        let config = parse("[router]\nhost = \"127.0.0.1:7331\"\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidHost)
        );
    }

    #[test]
    fn should_flag_invalid_dns_label() {
        let config = parse("[router]\nhost = \"-bad.example\"\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidHost)
        );
    }

    #[test]
    fn should_accept_ipv6_loopback() {
        let config = parse("[router]\nhost = \"::1\"\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(report.is_ok());
        assert!(report.warnings().is_empty());
    }

    #[test]
    fn should_not_warn_public_bind_in_remote_mode() {
        let config = parse(
            "[router]\nhost = \"0.0.0.0\"\n\n[router.storage]\nmode = \"remote\"\nurl = \"ws://db:8000\"\n",
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(report.warnings().is_empty());
    }

    #[test]
    fn should_warn_public_bind_in_embedded_mode() {
        let config = parse("[router]\nhost = \"0.0.0.0\"\n", "test").unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(
            report
                .warnings()
                .iter()
                .any(|e| e.code == ValidationCode::UnsafePublicBind)
        );
        assert!(report.is_ok());
    }

    #[test]
    fn should_accept_loopback_default() {
        let config = parse("", "test").unwrap();
        let mut report = ValidationReport::default();
        check_binding(&config, &mut report);
        assert!(report.is_ok());
        assert!(report.warnings().is_empty());
    }
}

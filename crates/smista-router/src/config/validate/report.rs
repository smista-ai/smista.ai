//! Validation report types for router runtime configuration.

use serde::Serialize;

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Blocks the action.
    Error,
    /// Surfaces but does not block.
    Warning,
}

/// Stable, machine-readable identity of a router validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCode {
    /// A timeout exceeds the sane upper bound.
    ExcessiveTimeout,
    /// A built-in provider is configured with a `base_url` that has no effect.
    IgnoredProviderBaseUrl,
    /// The bind host is empty or unparseable.
    InvalidHost,
    /// A size limit is zero.
    InvalidLimit,
    /// The bind port is zero.
    InvalidPort,
    /// A rate-limit setting is zero while rate limiting is enabled.
    InvalidRateLimit,
    /// A timeout is zero.
    InvalidTimeout,
    /// Local API-key bootstrap is enabled while storage mode is remote.
    LocalBootstrapInRemote,
    /// Storage config is missing a required path or url.
    MissingStorageConfig,
    /// CORS is enabled with unrestricted origins.
    UnsafeCors,
    /// A public bind address is used in embedded mode.
    UnsafePublicBind,
}

/// A single validation finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationError {
    /// Machine-readable code.
    pub code: ValidationCode,
    /// Severity.
    pub severity: Severity,
    /// Human-readable message: what is wrong and how to fix it. Never a secret.
    pub message: String,
    /// Dotted field path, e.g. `server.port`.
    pub location: Option<String>,
}

impl ValidationError {
    /// Renders the finding as a human-readable line, e.g. `error [server.port]: port 0 is not a valid bind port`.
    #[must_use]
    pub fn to_human(&self) -> String {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let location = self.location.as_deref().unwrap_or("-");
        format!("{severity} [{location}]: {}", self.message)
    }
}

/// The outcome of validating a router configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ValidationReport {
    errors: Vec<ValidationError>,
    warnings: Vec<ValidationError>,
}

impl ValidationReport {
    /// Records a finding, routing it by severity.
    pub fn push(&mut self, finding: ValidationError) {
        match finding.severity {
            Severity::Error => self.errors.push(finding),
            Severity::Warning => self.warnings.push(finding),
        }
    }

    /// Returns `true` when there are no errors (warnings are allowed).
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// All error-severity findings.
    #[must_use]
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// All warning-severity findings.
    #[must_use]
    pub fn warnings(&self) -> &[ValidationError] {
        &self.warnings
    }

    /// Renders findings as human-readable lines, errors first.
    #[must_use]
    pub fn to_human(&self) -> String {
        let mut out = String::new();
        for finding in self.errors.iter().chain(self.warnings.iter()) {
            out.push_str(&finding.to_human());
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_render_finding_with_location() {
        let finding = ValidationError {
            code: ValidationCode::InvalidPort,
            severity: Severity::Error,
            message: "port 0 is not a valid bind port".into(),
            location: Some("router.port".into()),
        };

        assert_eq!(
            finding.to_human(),
            "error [router.port]: port 0 is not a valid bind port"
        );
    }

    #[test]
    fn should_render_finding_without_location_as_dash() {
        let finding = ValidationError {
            code: ValidationCode::UnsafeCors,
            severity: Severity::Warning,
            message: "CORS allows any origin".into(),
            location: None,
        };

        assert_eq!(finding.to_human(), "warning [-]: CORS allows any origin");
    }

    #[test]
    fn should_render_human_lines() {
        let mut report = ValidationReport::default();
        report.push(ValidationError {
            code: ValidationCode::InvalidPort,
            severity: Severity::Error,
            message: "port 0 is not a valid bind port".into(),
            location: Some("router.port".into()),
        });
        let text = report.to_human();
        assert!(text.contains("error [router.port]: port 0 is not a valid bind port"));
    }

    #[test]
    fn should_route_findings_by_severity() {
        let mut report = ValidationReport::default();
        report.push(ValidationError {
            code: ValidationCode::InvalidPort,
            severity: Severity::Error,
            message: "port must not be zero".into(),
            location: None,
        });
        report.push(ValidationError {
            code: ValidationCode::ExcessiveTimeout,
            severity: Severity::Warning,
            message: "timeout exceeds recommended maximum".into(),
            location: None,
        });
        assert_eq!(report.errors().len(), 1);
        assert_eq!(report.warnings().len(), 1);
        assert!(!report.is_ok());
    }
}

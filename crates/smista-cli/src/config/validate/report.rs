//! Validation report types for CLI/policy configuration.

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

/// Stable, machine-readable identity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCode {
    /// Two routing rules share a `name`.
    DuplicateRule,
    /// A credential is an inline literal where a `${secret:..}` ref is required.
    InlineSecret,
    /// A fallback does not resolve, or a rule lists itself as a fallback.
    InvalidFallback,
    /// A glob pattern failed to compile.
    InvalidGlob,
    /// No `[routing.default]` is configured.
    MissingDefaultRoute,
    /// A preference layer widens a tool permission beyond the project default.
    PermissionWidening,
    /// Two rules share priority and specificity with no ordering allowance.
    RuleAmbiguity,
    /// A reference names a provider absent from `[providers]`.
    UnknownProvider,
    /// A preference layer weakens a non-overridable safety policy.
    UnsafeOverride,
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
    /// Dotted field path, e.g. `routing.rules[2].model`.
    pub location: Option<String>,
}

/// The outcome of validating a configuration.
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
            let severity = match finding.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            };
            let location = finding.location.as_deref().unwrap_or("-");
            out.push_str(&format!("{severity} [{location}]: {}\n", finding.message));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_route_findings_by_severity() {
        let mut report = ValidationReport::default();
        report.push(ValidationError {
            code: ValidationCode::UnknownProvider,
            severity: Severity::Error,
            message: "x".into(),
            location: None,
        });
        report.push(ValidationError {
            code: ValidationCode::PermissionWidening,
            severity: Severity::Warning,
            message: "y".into(),
            location: None,
        });
        assert_eq!(report.errors().len(), 1);
        assert_eq!(report.warnings().len(), 1);
        assert!(!report.is_ok());
    }

    #[test]
    fn should_render_human_lines() {
        let mut report = ValidationReport::default();
        report.push(ValidationError {
            code: ValidationCode::MissingDefaultRoute,
            severity: Severity::Error,
            message: "no default route".into(),
            location: Some("routing.default".into()),
        });
        let text = report.to_human();
        assert!(text.contains("error [routing.default]: no default route"));
    }
}

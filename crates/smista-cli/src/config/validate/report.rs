//! Validation report types for CLI/policy configuration.

use serde::Serialize;

/// Severity of a validation finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Blocks the action.
    Error,
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
    /// A runtime override widens a tool permission beyond the project default.
    PermissionWidening,
    /// Two rules share priority and specificity with no ordering allowance.
    RuleAmbiguity,
    /// A reference names a provider absent from `[providers]`.
    UnknownProvider,
    /// A runtime override weakens a non-overridable safety policy.
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
}

impl ValidationReport {
    /// Records a validation error.
    pub fn push(&mut self, finding: ValidationError) {
        self.errors.push(finding);
    }

    /// Returns `true` when there are no errors.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// All error-severity findings.
    #[must_use]
    pub fn errors(&self) -> &[ValidationError] {
        &self.errors
    }

    /// Renders findings as human-readable lines.
    #[must_use]
    pub fn to_human(&self) -> String {
        let mut out = String::new();
        for finding in &self.errors {
            let location = finding.location.as_deref().unwrap_or("-");
            out.push_str(&format!("error [{location}]: {}\n", finding.message));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_record_findings_as_errors() {
        let mut report = ValidationReport::default();
        report.push(ValidationError {
            code: ValidationCode::UnknownProvider,
            severity: Severity::Error,
            message: "x".into(),
            location: None,
        });
        assert_eq!(report.errors().len(), 1);
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

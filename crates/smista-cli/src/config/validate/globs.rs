//! Glob-pattern validity checks for privacy path lists.

use globset::Glob;

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;

/// Compiles every glob pattern in the privacy policy and rule paths; a compile
/// failure is the invalid-glob finding.
pub fn check_globs(config: &Config, report: &mut ValidationReport) {
    let lists = [
        ("privacy.restricted_paths", &config.privacy.restricted_paths),
        (
            "privacy.remote.blocked_paths",
            &config.privacy.remote.blocked_paths,
        ),
    ];
    let pattern_count: usize = lists
        .iter()
        .map(|(_, patterns)| patterns.len())
        .sum::<usize>()
        + config
            .routing
            .rules
            .iter()
            .map(|rule| rule.paths.len())
            .sum::<usize>();
    tracing::trace!(
        globs.pattern_count = pattern_count,
        "checking {{globs.pattern_count}} glob patterns"
    );
    for (field, patterns) in lists {
        for (index, pattern) in patterns.iter().enumerate() {
            if let Err(err) = compile(pattern) {
                let location = format!("{field}[{index}]");
                tracing::warn!(
                    globs.pattern = %pattern,
                    globs.location = %location,
                    error.message = %err,
                    "invalid glob pattern at {{globs.location}}"
                );
                report.push(ValidationError {
                    code: ValidationCode::InvalidGlob,
                    severity: Severity::Error,
                    message: format!("invalid glob `{pattern}`: {err}; fix the pattern syntax"),
                    location: Some(location),
                });
            }
        }
    }
    for (r, rule) in config.routing.rules.iter().enumerate() {
        for (index, pattern) in rule.paths.iter().enumerate() {
            if let Err(err) = compile(pattern) {
                let location = format!("routing.rules[{r}].paths[{index}]");
                tracing::warn!(
                    globs.pattern = %pattern,
                    globs.location = %location,
                    error.message = %err,
                    "invalid glob pattern at {{globs.location}}"
                );
                report.push(ValidationError {
                    code: ValidationCode::InvalidGlob,
                    severity: Severity::Error,
                    message: format!("invalid glob `{pattern}`: {err}; fix the pattern syntax"),
                    location: Some(location),
                });
            }
        }
    }
}

/// Compiles a single glob, returning the compile error on failure.
fn compile(pattern: &str) -> Result<(), globset::Error> {
    Glob::new(pattern).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_invalid_glob() {
        let config = parse(
            r#"
            [privacy]
            restricted_paths = ["src/**", "a[b"]
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_globs(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidGlob)
        );
    }

    #[test]
    fn should_accept_valid_globs() {
        let config = parse(
            r#"
            [privacy]
            restricted_paths = ["src/**", "*.pem", ".env"]
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_globs(&config, &mut report);
        assert!(report.is_ok());
    }
}

//! Structural routing-policy checks: rule identity, default route, fallbacks.

use std::collections::BTreeSet;

use smista_sdk::core::model::ModelReference;
use smista_sdk::core::policy::RoutingRule;

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::Config;

/// Validates the fallback list for a single route (either a named rule or the
/// default route), recording any self-reference or duplicate findings.
///
/// * `model` – the route's own model; a fallback equal to this is a
///   self-reference error.
/// * `fallbacks` – the fallback list to inspect.
/// * `label` – human-readable route label used in error messages
///   (e.g. `"rule \`foo\`"` or `"the default route"`).
/// * `location_prefix` – dotted path prefix for the location field
///   (e.g. `"routing.rules[0]"` or `"routing.default"`).
fn check_fallbacks(
    model: &ModelReference,
    fallbacks: &[ModelReference],
    label: &str,
    location_prefix: &str,
    report: &mut ValidationReport,
) {
    let mut seen = BTreeSet::new();
    for (f, fallback) in fallbacks.iter().enumerate() {
        let location = format!("{location_prefix}.fallbacks[{f}]");
        if fallback == model {
            tracing::warn!(
                routing.model = %model,
                routing.location = %location,
                "route lists its own model as a fallback at {{routing.location}}"
            );
            report.push(ValidationError {
                code: ValidationCode::InvalidFallback,
                severity: Severity::Error,
                message: format!("{label} lists its own model `{model}` as a fallback"),
                location: Some(location.clone()),
            });
        }
        if !seen.insert(fallback.to_string()) {
            tracing::warn!(
                routing.fallback = %fallback,
                routing.location = %location,
                "duplicate fallback {{routing.fallback}} at {{routing.location}}"
            );
            report.push(ValidationError {
                code: ValidationCode::InvalidFallback,
                severity: Severity::Error,
                message: format!("{label} lists duplicate fallback `{fallback}`"),
                location: Some(location),
            });
        }
    }
}

/// Checks routing structure independent of model existence:
/// duplicate rule names, a required default route, and fallback integrity
/// (no route lists its own `model` as a fallback, no duplicate fallbacks within
/// a route — applies to both named rules and the default route).
pub fn check_routing_structure(config: &Config, report: &mut ValidationReport) {
    if config.routing.default.is_none() {
        tracing::warn!("no default route configured");
        report.push(ValidationError {
            code: ValidationCode::MissingDefaultRoute,
            severity: Severity::Error,
            message: "no default route; add a [routing.default] table with a `model`".into(),
            location: Some("routing.default".into()),
        });
    }

    if let Some(default) = &config.routing.default {
        check_fallbacks(
            &default.model,
            &default.fallbacks,
            "the default route",
            "routing.default",
            report,
        );
    }

    let mut seen = BTreeSet::new();
    for (index, rule) in config.routing.rules.iter().enumerate() {
        if !seen.insert(rule.name.as_str()) {
            tracing::warn!(
                routing.rule = %rule.name,
                routing.location = %format!("routing.rules[{index}].name"),
                "duplicate routing rule name {{routing.rule}}"
            );
            report.push(ValidationError {
                code: ValidationCode::DuplicateRule,
                severity: Severity::Error,
                message: format!(
                    "duplicate routing rule name `{}`; rule names must be unique",
                    rule.name
                ),
                location: Some(format!("routing.rules[{index}].name")),
            });
        }

        check_fallbacks(
            &rule.model,
            &rule.fallbacks,
            &format!("rule `{}`", rule.name),
            &format!("routing.rules[{index}]"),
            report,
        );
    }
}

/// Local specificity score: number of constrained match dimensions.
///
/// **Placeholder.** This is a local approximation pending the canonical
/// `Specificity` definition from the routing spec (issue #5). When that lands,
/// reconcile this with the authoritative rule-precedence definition.
fn specificity(rule: &RoutingRule) -> u32 {
    u32::from(rule.intent.is_some())
        + u32::from(!rule.paths.is_empty())
        + u32::from(rule.local_only)
}

/// Returns whether two rules can match the same request.
fn rules_overlap(first: &RoutingRule, second: &RoutingRule) -> bool {
    first.intent.zip(second.intent).is_none_or(|(a, b)| a == b)
}

/// Flags overlapping rule pairs that share both `priority` and specificity,
/// which makes their relative order undefined.
pub fn check_rule_ambiguity(config: &Config, report: &mut ValidationReport) {
    let rules = &config.routing.rules;
    for (i, rule_i) in rules.iter().enumerate() {
        for (j, rule_j) in rules.iter().enumerate().skip(i + 1) {
            if rule_i.priority == rule_j.priority
                && specificity(rule_i) == specificity(rule_j)
                && rules_overlap(rule_i, rule_j)
            {
                tracing::warn!(
                    routing.rule_a = %rule_i.name,
                    routing.rule_b = %rule_j.name,
                    routing.priority = rule_i.priority,
                    "ambiguous rule pair {{routing.rule_a}} and {{routing.rule_b}} share priority and specificity"
                );
                report.push(ValidationError {
                    code: ValidationCode::RuleAmbiguity,
                    severity: Severity::Error,
                    message: format!(
                        "rules `{}` and `{}` share priority {} and specificity; \
                         give them distinct priorities",
                        rule_i.name, rule_j.name, rule_i.priority
                    ),
                    location: Some(format!("routing.rules[{j}].priority")),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_missing_default_route() {
        let config = parse("", "test").unwrap();
        let mut report = ValidationReport::default();
        check_routing_structure(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::MissingDefaultRoute)
        );
    }

    #[test]
    fn should_flag_duplicate_rule_names() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "dup"
            model = "openai/a"

            [[routing.rules]]
            name = "dup"
            model = "openai/b"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_routing_structure(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::DuplicateRule)
        );
    }

    #[test]
    fn should_flag_self_fallback() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "r"
            model = "openai/a"
            fallbacks = ["openai/a"]
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_routing_structure(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidFallback)
        );
    }

    #[test]
    fn should_flag_duplicate_fallbacks() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "r"
            model = "openai/a"
            fallbacks = ["openai/b", "openai/b"]
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_routing_structure(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidFallback)
        );
    }

    #[test]
    fn should_flag_default_route_self_fallback() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"
            fallbacks = ["openai/x"]
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_routing_structure(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::InvalidFallback)
        );
    }

    #[test]
    fn should_flag_equal_priority_and_specificity() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "a"
            priority = 10
            intent = "edit"
            model = "openai/a"

            [[routing.rules]]
            name = "b"
            priority = 10
            intent = "edit"
            model = "openai/b"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_rule_ambiguity(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::RuleAmbiguity)
        );
    }

    #[test]
    fn should_accept_same_priority_rules_with_different_intents() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "edit"
            priority = 10
            intent = "edit"
            model = "openai/a"

            [[routing.rules]]
            name = "review"
            priority = 10
            intent = "review"
            model = "openai/b"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_rule_ambiguity(&config, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_accept_same_priority_rules_with_disjoint_intents() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "plan"
            priority = 10
            intent = "plan"
            model = "openai/a"

            [[routing.rules]]
            name = "review"
            priority = 10
            intent = "review"
            model = "openai/b"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_rule_ambiguity(&config, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_flag_catch_all_rule_overlapping_specific_rule() {
        let config = parse(
            r#"
            [routing.default]
            model = "openai/x"

            [[routing.rules]]
            name = "catch all local"
            priority = 10
            local_only = true
            model = "openai/a"

            [[routing.rules]]
            name = "review local"
            priority = 10
            intent = "review"
            model = "openai/b"
            "#,
            "test",
        )
        .unwrap();
        let mut report = ValidationReport::default();
        check_rule_ambiguity(&config, &mut report);
        assert!(
            report
                .errors()
                .iter()
                .any(|e| e.code == ValidationCode::RuleAmbiguity)
        );
    }
}

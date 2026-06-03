//! Routing-rule skill reference checks.

use super::report::{Severity, ValidationCode, ValidationError, ValidationReport};
use crate::config::{Config, SkillStore};

/// Checks every routing rule's `skill` resolves to a discovered skill.
///
/// `skills` is the source of truth for available skill names. Pushes one
/// [`ValidationCode::UnknownSkill`] error per rule whose `skill` is set but does
/// not name a discovered skill. Rules without a `skill` condition are skipped.
pub fn check_skills(config: &Config, skills: &SkillStore, report: &mut ValidationReport) {
    for (index, rule) in config.routing.rules.iter().enumerate() {
        let Some(skill) = &rule.skill else {
            continue;
        };
        if !skills.contains(skill) {
            let location = format!("routing.rules[{index}].skill");
            tracing::warn!(
                skills.name = %skill,
                skills.location = %location,
                "unresolved skill reference at {{skills.location}}"
            );
            report.push(ValidationError {
                code: ValidationCode::UnknownSkill,
                severity: Severity::Error,
                message: format!(
                    "skill `{skill}` is not a discovered skill; create a `{skill}` skill or correct the rule's `skill`"
                ),
                location: Some(location),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    #[test]
    fn should_flag_rule_referencing_unknown_skill() {
        let config = parse(
            r#"
            [[routing.rules]]
            name = "changelog rule"
            skill = "changelog"
            model = "ollama/llama3"
            "#,
            "test",
        )
        .unwrap();
        let skills = SkillStore::from_names(["review"]);
        let mut report = ValidationReport::default();
        check_skills(&config, &skills, &mut report);
        let error = report
            .errors()
            .iter()
            .find(|e| e.code == ValidationCode::UnknownSkill)
            .expect("unknown skill error");
        assert_eq!(error.location.as_deref(), Some("routing.rules[0].skill"));
    }

    #[test]
    fn should_pass_when_skill_is_discovered() {
        let config = parse(
            r#"
            [[routing.rules]]
            name = "changelog rule"
            skill = "changelog"
            model = "ollama/llama3"
            "#,
            "test",
        )
        .unwrap();
        let skills = SkillStore::from_names(["changelog"]);
        let mut report = ValidationReport::default();
        check_skills(&config, &skills, &mut report);
        assert!(report.is_ok());
    }

    #[test]
    fn should_ignore_rules_without_a_skill_condition() {
        let config = parse(
            r#"
            [[routing.rules]]
            name = "no skill"
            intent = "edit"
            model = "ollama/llama3"
            "#,
            "test",
        )
        .unwrap();
        let skills = SkillStore::from_names(Vec::<String>::new());
        let mut report = ValidationReport::default();
        check_skills(&config, &skills, &mut report);
        assert!(report.is_ok());
    }
}

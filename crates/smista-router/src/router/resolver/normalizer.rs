//! Deterministic task normalization: the resolver's first stage.
//!
//! [`TaskNormalizer`] turns the observable signals of one turn — the prompt, the
//! explicit command, the workspace snapshot, the attached skills and the user's
//! [`ClassificationConfig`] — into a [`NormalizedTask`]: the classified
//! [`TaskIntent`] (with provenance), the relevant skills, and the touched files.
//! That is the input the routing policy matcher consumes to pick a model.
//!
//! Normalization is **purely deterministic and never calls an LLM**, the core
//! invariant of smista.ai: the same inputs always produce the same result. The
//! work splits across two child modules — [`fuzzy`] for typo-tolerant keyword
//! matching and [`signals`] for the workspace- and skill-derived inputs — while
//! this module owns the classification decision itself.
//!
//! See `docs/technical/task-classification.md` for the user-facing description.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired by the policy matcher (#141) and orchestrator (#148) later"
    )
)]

mod fuzzy;
mod signals;

use std::collections::HashSet;
use std::path::PathBuf;

use smista_core::api::{TaskInput, Workspace};
use smista_core::intent::TaskIntent;
use smista_core::policy::{
    Classification, ClassificationConfig, ClassificationRule, Confidence, IntentSource, RoutingRule,
};
use smista_core::skill::Skill;

use self::fuzzy::KeywordHit;

/// The normalized form of one turn's request.
///
/// Produced by [`TaskNormalizer::normalize`]. It bundles the canonical
/// [`Classification`] (so the trace and the turn response keep the full intent
/// provenance) with the two signals the routing policy matches on: the
/// [relevant skills](Self::skills) and the [touched files](Self::touched_files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTask {
    /// The deterministic classification outcome: intent plus provenance.
    pub classification: Classification,
    /// Client-supplied skills judged relevant to this turn.
    pub skills: Vec<Skill>,
    /// File paths relevant to the task, matched against routing-rule path globs.
    pub touched_files: Vec<PathBuf>,
}

impl NormalizedTask {
    /// The classified task intent.
    #[must_use]
    pub fn intent(&self) -> TaskIntent {
        self.classification.intent
    }

    /// Returns `true` when every present condition of `rule` holds for this task.
    ///
    /// Conditions across fields are AND-combined; the path-glob list is
    /// OR-combined; absent conditions are ignored, so a rule with no conditions
    /// matches everything. A rule's [`skill`](RoutingRule::skill) matches when a
    /// skill of that name is among the [relevant skills](Self::skills). Invalid
    /// path globs match nothing rather than panicking.
    #[must_use]
    pub fn matches(&self, rule: &RoutingRule) -> bool {
        if let Some(intent) = rule.intent
            && self.intent() != intent
        {
            return false;
        }
        if let Some(skill) = &rule.skill
            && !self.skills.iter().any(|relevant| relevant.name == *skill)
        {
            return false;
        }
        if !rule.paths.is_empty() && !signals::paths_match(&rule.paths, &self.touched_files) {
            return false;
        }
        true
    }
}

/// Deterministic, LLM-free classifier and signal extractor for one turn.
///
/// Stateless: it reads only the per-request inputs passed to
/// [`normalize`](Self::normalize).
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskNormalizer;

impl TaskNormalizer {
    /// Normalizes one turn into a [`NormalizedTask`].
    ///
    /// `input` carries the prompt and an optional explicit command; `workspace`
    /// supplies the context kinds and touched files; `skills` are the attached
    /// skills; `config` holds the classification rules and default intent. The
    /// result is always defined — classification falls back to
    /// [`ClassificationConfig::default_intent`] when no rule matches.
    #[must_use]
    pub fn normalize(
        &self,
        input: &TaskInput,
        workspace: &Workspace,
        skills: &[Skill],
        config: &ClassificationConfig,
    ) -> NormalizedTask {
        let tokens = fuzzy::tokenize(&input.text);
        NormalizedTask {
            classification: self.classify(input, &tokens, workspace, config),
            skills: signals::relevant_skills(skills, &tokens),
            touched_files: signals::touched_files(workspace),
        }
    }

    /// Classifies the turn into a [`Classification`].
    ///
    /// An explicit command wins outright. Otherwise the rules are tried in ascending
    /// priority (configuration order breaking ties) and the first match wins,
    /// falling back to the default intent when none do.
    fn classify(
        &self,
        input: &TaskInput,
        tokens: &[String],
        workspace: &Workspace,
        config: &ClassificationConfig,
    ) -> Classification {
        if let Some(intent) = input.command {
            return Classification {
                intent,
                source: IntentSource::Explicit,
                reason: format!("explicit command '{intent}'"),
                matched_rule: None,
                confidence: None,
            };
        }

        let contexts = signals::context_kinds(workspace);
        for index in self.rules_by_priority(config) {
            if let Some(classification) =
                self.match_rule(&config.rules[index], index, tokens, &contexts)
            {
                return classification;
            }
        }

        Classification {
            intent: config.default_intent,
            source: IntentSource::Inferred,
            reason: format!(
                "no rule matched; default intent '{}'",
                config.default_intent
            ),
            matched_rule: None,
            confidence: Some(Confidence::Low),
        }
    }

    /// Rule indices ordered by ascending priority, ties broken by configuration
    /// order so the reported `matched_rule` stays the original index.
    fn rules_by_priority(&self, config: &ClassificationConfig) -> Vec<usize> {
        let mut order: Vec<usize> = (0..config.rules.len()).collect();
        order.sort_by(|&a, &b| {
            config.rules[a]
                .priority
                .cmp(&config.rules[b].priority)
                .then(a.cmp(&b))
        });
        order
    }

    /// Evaluates one rule, yielding its [`Classification`] only when it matches.
    ///
    /// A rule matches when each *present* condition holds: any keyword matches a
    /// prompt token, and any required context kind is available. A missing required
    /// condition short-circuits to `None` via `?`.
    fn match_rule(
        &self,
        rule: &ClassificationRule,
        index: usize,
        tokens: &[String],
        contexts: &HashSet<&str>,
    ) -> Option<Classification> {
        let keyword_required = !rule.keywords.is_empty();
        let context_required = !rule.requires_any_context.is_empty();

        let keyword_hit = if keyword_required {
            Some(fuzzy::match_keyword(&rule.keywords, tokens)?)
        } else {
            None
        };
        let context_hit = if context_required {
            Some(self.first_context_hit(&rule.requires_any_context, contexts)?)
        } else {
            None
        };

        Some(Classification {
            intent: rule.intent,
            source: IntentSource::Inferred,
            reason: self.match_reason(index, keyword_hit.as_ref(), context_hit),
            matched_rule: Some(index),
            confidence: Some(self.confidence_for(
                keyword_required,
                context_required,
                keyword_hit.as_ref(),
            )),
        })
    }

    /// Finds the first required context kind that is available.
    fn first_context_hit<'a>(
        &self,
        required: &'a [String],
        available: &HashSet<&str>,
    ) -> Option<&'a String> {
        required
            .iter()
            .find(|kind| available.contains(kind.as_str()))
    }

    /// Derives the confidence of a matched rule.
    ///
    /// See the confidence table in `docs/technical/task-classification.md`. A rule
    /// matching both condition kinds is `high`, unless the keyword matched only
    /// through a typo, which caps it at `medium`.
    fn confidence_for(
        &self,
        keyword_required: bool,
        context_required: bool,
        keyword_hit: Option<&KeywordHit>,
    ) -> Confidence {
        match (keyword_required, context_required) {
            (true, true) if keyword_hit.is_some_and(|hit| hit.fuzzy) => Confidence::Medium,
            (true, true) => Confidence::High,
            (true, false) | (false, true) => Confidence::Medium,
            (false, false) => Confidence::Low,
        }
    }

    /// Builds the human-readable match reason for rule `index`.
    fn match_reason(
        &self,
        index: usize,
        keyword_hit: Option<&KeywordHit>,
        context_hit: Option<&String>,
    ) -> String {
        match (keyword_hit, context_hit) {
            (Some(keyword), Some(context)) => format!(
                "keyword '{}' and context '{context}' matched rule {index}",
                keyword.keyword
            ),
            (Some(keyword), None) => format!("keyword '{}' matched rule {index}", keyword.keyword),
            (None, Some(context)) => format!("context '{context}' matched rule {index}"),
            (None, None) => format!("catch-all rule {index} matched"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a workspace with no signals; tests opt into the parts they need.
    fn workspace() -> Workspace {
        Workspace {
            root: PathBuf::from("/repo"),
            git_branch: None,
            git_diff: None,
            referenced_paths: Vec::new(),
            active_file: None,
        }
    }

    fn input(text: &str) -> TaskInput {
        TaskInput {
            text: text.to_string(),
            command: None,
            explicit_model: None,
        }
    }

    fn rule(value: serde_json::Value) -> ClassificationRule {
        serde_json::from_value(value).expect("valid classification rule")
    }

    fn config(rules: Vec<serde_json::Value>) -> ClassificationConfig {
        ClassificationConfig {
            default_intent: TaskIntent::Chat,
            rules: rules.into_iter().map(rule).collect(),
        }
    }

    fn skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: format!("{name} skill"),
            instructions: "do the thing".to_string(),
        }
    }

    #[test]
    fn should_let_explicit_command_win_over_rules() {
        let mut request = input("review my changes");
        request.command = Some(TaskIntent::Edit);
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review"],
        })]);

        let task = TaskNormalizer.normalize(&request, &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Edit);
        assert_eq!(task.classification.source, IntentSource::Explicit);
        assert_eq!(task.classification.matched_rule, None);
        assert_eq!(task.classification.confidence, None);
    }

    #[test]
    fn should_match_keyword_only_rule_with_medium_confidence() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review", "audit"],
        })]);

        let task = TaskNormalizer.normalize(&input("please review this"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Review);
        assert_eq!(task.classification.source, IntentSource::Inferred);
        assert_eq!(task.classification.matched_rule, Some(0));
        assert_eq!(task.classification.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn should_match_context_only_rule_with_medium_confidence() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "requires_any_context": ["git_diff"],
        })]);
        let mut ws = workspace();
        ws.git_diff = Some("diff --git a/x b/x".to_string());

        let task = TaskNormalizer.normalize(&input("anything"), &ws, &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Review);
        assert_eq!(task.classification.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn should_match_both_conditions_with_high_confidence() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review"],
            "requires_any_context": ["git_diff"],
        })]);
        let mut ws = workspace();
        ws.git_diff = Some("diff --git a/x b/x".to_string());

        let task = TaskNormalizer.normalize(&input("review my changes"), &ws, &[], &cfg);

        assert_eq!(task.classification.confidence, Some(Confidence::High));
        assert!(task.classification.reason.contains("git_diff"));
    }

    #[test]
    fn should_not_match_rule_when_required_context_is_absent() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review"],
            "requires_any_context": ["git_diff"],
        })]);

        // "review this idea" with no diff available, mirroring the spec example.
        let task = TaskNormalizer.normalize(&input("review this idea"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Chat);
        assert_eq!(task.classification.matched_rule, None);
        assert_eq!(task.classification.confidence, Some(Confidence::Low));
    }

    #[test]
    fn should_fall_back_to_default_intent_when_no_rule_matches() {
        let cfg = config(vec![serde_json::json!({
            "intent": "edit",
            "keywords": ["refactor"],
        })]);

        let task = TaskNormalizer.normalize(&input("what does this do?"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Chat);
        assert_eq!(task.classification.source, IntentSource::Inferred);
        assert_eq!(task.classification.confidence, Some(Confidence::Low));
    }

    #[test]
    fn should_match_conditionless_catch_all_with_low_confidence() {
        let cfg = config(vec![serde_json::json!({ "intent": "plan" })]);

        let task = TaskNormalizer.normalize(&input("anything at all"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Plan);
        assert_eq!(task.classification.confidence, Some(Confidence::Low));
    }

    #[test]
    fn should_pick_first_rule_by_priority_keeping_original_index() {
        let cfg = config(vec![
            serde_json::json!({ "intent": "plan", "priority": 100, "keywords": ["go"] }),
            serde_json::json!({ "intent": "edit", "priority": 10, "keywords": ["go"] }),
        ]);

        let task = TaskNormalizer.normalize(&input("go"), &workspace(), &[], &cfg);

        // The lower-priority-value rule (index 1) wins; its original index is kept.
        assert_eq!(task.classification.intent, TaskIntent::Edit);
        assert_eq!(task.classification.matched_rule, Some(1));
    }

    #[test]
    fn should_break_priority_ties_on_configuration_order() {
        let cfg = config(vec![
            serde_json::json!({ "intent": "plan", "priority": 10, "keywords": ["go"] }),
            serde_json::json!({ "intent": "edit", "priority": 10, "keywords": ["go"] }),
        ]);

        let task = TaskNormalizer.normalize(&input("go"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Plan);
        assert_eq!(task.classification.matched_rule, Some(0));
    }

    #[test]
    fn should_match_keyword_through_a_typo_and_cap_confidence_at_medium() {
        let cfg = config(vec![serde_json::json!({
            "intent": "edit",
            "keywords": ["implement"],
            "requires_any_context": ["git_diff"],
        })]);
        let mut ws = workspace();
        ws.git_diff = Some("diff --git a/x b/x".to_string());

        // "impelment" is one OSA transposition from "implement".
        let task = TaskNormalizer.normalize(&input("impelment the feature"), &ws, &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Edit);
        // Both conditions held, but the fuzzy keyword caps the signal.
        assert_eq!(task.classification.confidence, Some(Confidence::Medium));
    }

    #[test]
    fn should_require_exact_match_for_short_keywords() {
        let cfg = config(vec![serde_json::json!({
            "intent": "edit",
            "keywords": ["edit"],
        })]);

        // "audit" is one edit from "edit" but the four-character keyword bucket
        // is exact-only, so it must not match.
        let task = TaskNormalizer.normalize(&input("audit the code"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Chat);
    }

    #[test]
    fn should_reject_two_typos_in_the_medium_length_bucket() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review"],
        })]);

        // "reviXX" is distance two from the six-character keyword (cap one).
        let task = TaskNormalizer.normalize(&input("revixx the code"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Chat);
    }

    #[test]
    fn should_not_fuzzy_match_unrelated_word_with_keyword_suffix() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review"],
        })]);

        let task = TaskNormalizer.normalize(&input("preview the page"), &workspace(), &[], &cfg);

        assert_eq!(task.classification.intent, TaskIntent::Chat);
    }

    #[test]
    fn should_be_deterministic_across_calls() {
        let cfg = config(vec![serde_json::json!({
            "intent": "review",
            "keywords": ["review"],
        })]);
        let request = input("review please");

        let first = TaskNormalizer.normalize(&request, &workspace(), &[], &cfg);
        let second = TaskNormalizer.normalize(&request, &workspace(), &[], &cfg);

        assert_eq!(first, second);
    }

    #[test]
    fn should_collect_touched_files_deduped_in_stable_order() {
        let mut ws = workspace();
        ws.referenced_paths = vec![PathBuf::from("src/auth/login.rs")];
        ws.active_file = Some(PathBuf::from("src/auth/login.rs"));
        ws.git_diff = Some(
            "diff --git a/src/main.rs b/src/main.rs\n+++ b/src/main.rs\ndiff --git a/Cargo.toml b/Cargo.toml"
                .to_string(),
        );

        let task =
            TaskNormalizer.normalize(&input("go"), &ws, &[], &ClassificationConfig::default());

        assert_eq!(
            task.touched_files,
            [
                PathBuf::from("src/auth/login.rs"),
                PathBuf::from("src/main.rs"),
                PathBuf::from("Cargo.toml"),
            ]
        );
    }

    #[test]
    fn should_select_skills_matching_a_prompt_token() {
        let skills = vec![skill("changelog"), skill("security-review")];
        let cfg = ClassificationConfig::default();

        let task =
            TaskNormalizer.normalize(&input("update the changelog"), &workspace(), &skills, &cfg);

        assert_eq!(task.skills.len(), 1);
        assert_eq!(task.skills[0].name, "changelog");
    }

    #[test]
    fn should_select_hyphenated_skill_matching_prompt_words() {
        let skills = vec![skill("security-review"), skill("changelog")];
        let cfg = ClassificationConfig::default();

        let task =
            TaskNormalizer.normalize(&input("run a security review"), &workspace(), &skills, &cfg);

        assert_eq!(task.skills.len(), 1);
        assert_eq!(task.skills[0].name, "security-review");
    }

    #[test]
    fn should_select_no_skills_when_none_are_named() {
        let skills = vec![skill("changelog")];
        let cfg = ClassificationConfig::default();

        let task = TaskNormalizer.normalize(&input("fix the bug"), &workspace(), &skills, &cfg);

        assert!(task.skills.is_empty());
    }

    fn routing_rule(value: serde_json::Value) -> RoutingRule {
        serde_json::from_value(value).expect("valid routing rule")
    }

    fn task_with(intent: TaskIntent, skills: Vec<Skill>, touched: Vec<&str>) -> NormalizedTask {
        NormalizedTask {
            classification: Classification {
                intent,
                source: IntentSource::Inferred,
                reason: "test".to_string(),
                matched_rule: None,
                confidence: Some(Confidence::Low),
            },
            skills,
            touched_files: touched.into_iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn should_match_conditionless_routing_rule() {
        let rule = routing_rule(serde_json::json!({ "name": "any", "model": "ollama/llama3" }));
        let task = task_with(TaskIntent::Chat, vec![], vec!["src/main.rs"]);
        assert!(task.matches(&rule));
    }

    #[test]
    fn should_match_routing_rule_on_intent_and_path() {
        let rule = routing_rule(serde_json::json!({
            "name": "auth edits",
            "intent": "edit",
            "paths": ["src/auth/**"],
            "model": "ollama/llama3",
        }));

        let hit = task_with(TaskIntent::Edit, vec![], vec!["src/auth/login.rs"]);
        assert!(hit.matches(&rule));

        let wrong_intent = task_with(TaskIntent::Review, vec![], vec!["src/auth/login.rs"]);
        assert!(!wrong_intent.matches(&rule));

        let wrong_path = task_with(TaskIntent::Edit, vec![], vec!["docs/readme.md"]);
        assert!(!wrong_path.matches(&rule));
    }

    #[test]
    fn should_match_routing_rule_skill_against_the_relevant_set() {
        let rule = routing_rule(serde_json::json!({
            "name": "changelog skill",
            "skill": "changelog",
            "model": "ollama/llama3",
        }));

        let hit = task_with(
            TaskIntent::Chat,
            vec![skill("changelog"), skill("security-review")],
            vec![],
        );
        assert!(hit.matches(&rule));

        let miss = task_with(TaskIntent::Chat, vec![skill("security-review")], vec![]);
        assert!(!miss.matches(&rule));
    }

    #[test]
    fn should_treat_invalid_routing_globs_as_no_match() {
        let mut rule = routing_rule(serde_json::json!({ "name": "bad", "model": "ollama/llama3" }));
        rule.paths = vec!["[".to_string()];

        let task = task_with(TaskIntent::Chat, vec![], vec!["src/main.rs"]);
        assert!(!task.matches(&rule));
    }
}

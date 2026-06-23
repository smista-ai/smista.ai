//! Routing-policy matching: the resolver's second stage.
//!
//! [`PolicyMatcher`] takes the [`NormalizedTask`] produced by the
//! [normalizer](super::normalizer) and the user's [`RoutingPolicy`] and picks
//! exactly one route for the turn — an explicit model override, a matched
//! [`RoutingRule`], or the policy's default route — returning a [`RouteMatch`]
//! that names the chosen model and explains the decision.
//!
//! Matching is **purely deterministic and never calls an LLM**, the core
//! invariant of smista.ai. It also never consults a provider: availability,
//! capability gating, fallback walking and locality belong to model selection,
//! which reads the [`RouteMatch`] this stage emits.
//!
//! The precedence rules are described, user-facing, in
//! `docs/technical/routing.md`.

use smista_core::intent::TaskIntent;
use smista_core::model::ModelReference;
use smista_core::policy::{DefaultRoute, RoutingPolicy, RoutingRule, Specificity};

use crate::router::resolver::normalizer::NormalizedTask;

/// The error returned when a task cannot be routed.
///
/// The matcher never guesses a model: when no rule matches and the policy
/// declares no default route, routing fails loud with this error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyMatchError {
    /// No rule matched and the policy declares no default route.
    #[error("no routing rule matched and the policy declares no default route")]
    NoRoute,
}

/// Where a [`RouteMatch`] came from, kept for the trace.
///
/// The three variants mirror the precedence ladder: an explicit override wins
/// outright, otherwise a matched rule, otherwise the default route. Each borrows
/// the chosen model (and any fallback chain) from the inputs rather than cloning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSource<'a> {
    /// An explicit `input.explicit_model` override bypassed rule matching.
    Override {
        /// The model the user pinned for the turn.
        model: &'a ModelReference,
    },
    /// The rule at this index in [`RoutingPolicy::rules`] matched.
    Rule {
        /// Index of the matched rule in the policy's rule list.
        index: usize,
        /// The matched rule, carrying its model, effort and permissions.
        rule: &'a RoutingRule,
    },
    /// No rule matched, so the default route was used.
    Default {
        /// The policy's default route.
        route: &'a DefaultRoute,
    },
}

/// The deterministic outcome of matching the routing policy to a task.
///
/// Names the [intent](Self::intent) that drove routing, the
/// [source](Self::source) of the decision and a human-readable
/// [reason](Self::reason). The selected [model](Self::model) and its
/// [fallback chain](Self::fallbacks) are read from the source; capability,
/// availability and locality are resolved by the model-selection stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteMatch<'a> {
    /// The task intent that drove routing.
    pub intent: TaskIntent,
    /// Where the route came from: an override, a rule, or the default route.
    pub source: RouteSource<'a>,
    /// Human-readable explanation of why this route was chosen.
    pub reason: String,
}

impl<'a> RouteMatch<'a> {
    /// The primary model the turn should be served by.
    #[must_use]
    pub fn model(&self) -> &'a ModelReference {
        match self.source {
            RouteSource::Override { model } => model,
            RouteSource::Rule { rule, .. } => &rule.model,
            RouteSource::Default { route } => &route.model,
        }
    }

    /// The ordered fallback chain for the primary model.
    ///
    /// An override has no fallback chain — it is the user's deliberate choice —
    /// so this is empty for [`RouteSource::Override`].
    #[must_use]
    pub fn fallbacks(&self) -> &'a [ModelReference] {
        match self.source {
            RouteSource::Override { .. } => &[],
            RouteSource::Rule { rule, .. } => &rule.fallbacks,
            RouteSource::Default { route } => &route.fallbacks,
        }
    }

    /// The matched rule, or `None` for an override or the default route.
    #[must_use]
    pub fn matched_rule(&self) -> Option<&'a RoutingRule> {
        match self.source {
            RouteSource::Rule { rule, .. } => Some(rule),
            RouteSource::Override { .. } | RouteSource::Default { .. } => None,
        }
    }

    /// Whether an explicit model override was used.
    #[must_use]
    pub fn override_used(&self) -> bool {
        matches!(self.source, RouteSource::Override { .. })
    }
}

/// Deterministic, LLM-free matcher of a routing policy to a normalized task.
///
/// Stateless: it reads only the per-request inputs passed to
/// [`match_policy`](Self::match_policy).
#[derive(Debug, Default, Clone, Copy)]
pub struct PolicyMatcher;

impl PolicyMatcher {
    /// Picks exactly one route for `task` against `policy`.
    ///
    /// Resolution follows a fixed precedence. An `override_model` (the request's
    /// `input.explicit_model`) wins outright and bypasses rule matching.
    /// Otherwise the rules are evaluated in ascending priority, ties broken by
    /// descending [`Specificity`] and then by configuration order, and the first
    /// rule whose conditions hold is chosen. When no rule matches, the policy's
    /// default route is used.
    ///
    /// No provider is consulted and no model is called: this stage only decides
    /// *which* route applies, leaving availability and locality to selection.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyMatchError::NoRoute`] when no rule matches and the policy
    /// declares no default route.
    pub fn match_policy<'a>(
        &self,
        task: &NormalizedTask,
        policy: &'a RoutingPolicy,
        override_model: Option<&'a ModelReference>,
    ) -> Result<RouteMatch<'a>, PolicyMatchError> {
        let intent = task.intent();

        if let Some(model) = override_model {
            tracing::debug!(%intent, %model, "routed by explicit model override");
            return Ok(RouteMatch {
                intent,
                reason: format!("explicit model override '{model}'"),
                source: RouteSource::Override { model },
            });
        }

        if let Some((index, rule)) = self.first_matching_rule(task, policy) {
            tracing::debug!(
                %intent,
                rule = index,
                rule.name = %rule.name,
                model = %rule.model,
                "routed by matched rule"
            );
            return Ok(RouteMatch {
                intent,
                reason: rule_reason(rule),
                source: RouteSource::Rule { index, rule },
            });
        }

        match policy.default.as_ref() {
            Some(route) => {
                tracing::debug!(%intent, model = %route.model, "routed by default route");
                Ok(RouteMatch {
                    intent,
                    reason: format!(
                        "no rule matched; using the default route to '{}'",
                        route.model
                    ),
                    source: RouteSource::Default { route },
                })
            }
            None => {
                tracing::warn!(%intent, "no rule matched and the policy declares no default route");
                Err(PolicyMatchError::NoRoute)
            }
        }
    }

    /// Returns the highest-precedence rule whose conditions hold, with its index.
    ///
    /// Rules are ordered by ascending priority, then descending specificity, then
    /// configuration order, matching [`RoutingRule::precedence_cmp`]; the first
    /// match in that order wins.
    fn first_matching_rule<'a>(
        &self,
        task: &NormalizedTask,
        policy: &'a RoutingPolicy,
    ) -> Option<(usize, &'a RoutingRule)> {
        let mut order: Vec<usize> = (0..policy.rules.len()).collect();
        order.sort_by(|&a, &b| {
            RoutingRule::precedence_cmp(&policy.rules[a], &policy.rules[b]).then(a.cmp(&b))
        });
        order
            .into_iter()
            .map(|index| (index, &policy.rules[index]))
            .find(|&(_, rule)| task.matches(rule))
    }
}

/// Builds the human-readable reason a rule matched, from its specificity.
fn rule_reason(rule: &RoutingRule) -> String {
    let name = &rule.name;
    match rule.specificity() {
        Specificity::PathIntent => format!("rule '{name}' matched on intent and path"),
        Specificity::Path => format!("rule '{name}' matched on path"),
        Specificity::Intent => format!("rule '{name}' matched on intent"),
        Specificity::Default => format!("rule '{name}' matched as a catch-all"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;
    use smista_core::policy::{Classification, Confidence, IntentSource};

    use super::*;

    fn task_with(intent: TaskIntent, touched: &[&str]) -> NormalizedTask {
        NormalizedTask {
            classification: Classification {
                intent,
                source: IntentSource::Inferred,
                reason: "test".to_string(),
                matched_rule: None,
                confidence: Some(Confidence::Low),
            },
            touched_files: touched.iter().map(PathBuf::from).collect(),
        }
    }

    fn policy(value: serde_json::Value) -> RoutingPolicy {
        serde_json::from_value(value).expect("valid routing policy")
    }

    fn model(reference: &str) -> ModelReference {
        reference.parse().expect("valid model reference")
    }

    #[test]
    fn should_use_explicit_override_bypassing_rules() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{ "name": "edits", "intent": "edit", "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);
        let pinned = model("anthropic/claude-sonnet");

        let matched = PolicyMatcher
            .match_policy(&task, &routing, Some(&pinned))
            .expect("an override always routes");

        assert!(matched.override_used());
        assert_eq!(matched.model(), &pinned);
        assert!(matched.fallbacks().is_empty());
        assert!(matched.matched_rule().is_none());
        assert_eq!(matched.intent, TaskIntent::Edit);
        assert!(matched.reason.contains("override"));
    }

    #[test]
    fn should_match_rule_on_intent_and_path() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{
                "name": "auth edits use Claude",
                "intent": "edit",
                "paths": ["src/auth/**"],
                "model": "anthropic/claude-sonnet",
                "fallbacks": ["openai/gpt-5.5-thinking"],
            }],
        }));
        let task = task_with(TaskIntent::Edit, &["src/auth/login.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert!(!matched.override_used());
        assert_eq!(matched.model(), &model("anthropic/claude-sonnet"));
        assert_eq!(
            matched.fallbacks().to_vec(),
            vec![model("openai/gpt-5.5-thinking")]
        );
        assert_eq!(
            matched.matched_rule().unwrap().name,
            "auth edits use Claude"
        );
        assert_eq!(
            matched.reason,
            "rule 'auth edits use Claude' matched on intent and path"
        );
        assert!(matches!(matched.source, RouteSource::Rule { index: 0, .. }));
    }

    #[test]
    fn should_fall_back_to_default_route_when_no_rule_matches() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini", "fallbacks": ["ollama/qwen2.5-coder"] },
            "rules": [{ "name": "auth", "paths": ["src/auth/**"], "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Chat, &["docs/readme.md"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert!(matches!(matched.source, RouteSource::Default { .. }));
        assert_eq!(matched.model(), &model("openai/gpt-5.5-mini"));
        assert_eq!(
            matched.fallbacks().to_vec(),
            vec![model("ollama/qwen2.5-coder")]
        );
        assert!(matched.matched_rule().is_none());
        assert!(!matched.override_used());
        assert!(matched.reason.contains("default route"));
    }

    #[test]
    fn should_error_when_no_rule_matches_and_no_default() {
        let routing = policy(json!({
            "rules": [{ "name": "auth", "paths": ["src/auth/**"], "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Chat, &["docs/readme.md"]);

        let result = PolicyMatcher.match_policy(&task, &routing, None);

        assert_eq!(result, Err(PolicyMatchError::NoRoute));
    }

    #[test]
    fn should_pick_lower_priority_value_first() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [
                { "name": "late", "priority": 100, "intent": "edit", "model": "ollama/late" },
                { "name": "early", "priority": 10, "intent": "edit", "model": "ollama/early" },
            ],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(matched.matched_rule().unwrap().name, "early");
        assert!(matches!(matched.source, RouteSource::Rule { index: 1, .. }));
    }

    #[test]
    fn should_break_priority_ties_by_specificity() {
        // Both match at priority 10; the path+intent rule outranks intent-only.
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [
                { "name": "intent only", "priority": 10, "intent": "edit", "model": "ollama/broad" },
                {
                    "name": "path and intent",
                    "priority": 10,
                    "intent": "edit",
                    "paths": ["src/**"],
                    "model": "ollama/specific",
                },
            ],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(matched.matched_rule().unwrap().name, "path and intent");
    }

    #[test]
    fn should_break_full_ties_by_configuration_order() {
        // Equal priority and specificity: the earlier rule in the config wins.
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [
                { "name": "first", "priority": 10, "intent": "edit", "model": "ollama/first" },
                { "name": "second", "priority": 10, "intent": "edit", "model": "ollama/second" },
            ],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(matched.matched_rule().unwrap().name, "first");
        assert!(matches!(matched.source, RouteSource::Rule { index: 0, .. }));
    }

    #[test]
    fn should_skip_higher_precedence_rule_that_does_not_match() {
        // The lower-priority rule does not match this path, so the next wins.
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [
                { "name": "auth only", "priority": 10, "paths": ["src/auth/**"], "model": "ollama/auth" },
                { "name": "any edit", "priority": 20, "intent": "edit", "model": "ollama/edit" },
            ],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(matched.matched_rule().unwrap().name, "any edit");
    }

    #[test]
    fn should_match_conditionless_catch_all() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{ "name": "catch all", "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Chat, &["anything.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(matched.matched_rule().unwrap().name, "catch all");
        assert_eq!(matched.reason, "rule 'catch all' matched as a catch-all");
    }

    #[test]
    fn should_describe_path_only_and_intent_only_matches() {
        let path_only = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{ "name": "paths", "paths": ["src/**"], "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Chat, &["src/main.rs"]);
        let matched = PolicyMatcher.match_policy(&task, &path_only, None).unwrap();
        assert_eq!(matched.reason, "rule 'paths' matched on path");

        let intent_only = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{ "name": "kind", "intent": "review", "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Review, &["src/main.rs"]);
        let matched = PolicyMatcher
            .match_policy(&task, &intent_only, None)
            .unwrap();
        assert_eq!(matched.reason, "rule 'kind' matched on intent");
    }

    #[test]
    fn should_let_override_win_even_when_a_rule_matches() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{
                "name": "edits",
                "intent": "edit",
                "model": "ollama/llama3",
                "fallbacks": ["ollama/backup"],
            }],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);
        let pinned = model("anthropic/claude-sonnet");

        let matched = PolicyMatcher
            .match_policy(&task, &routing, Some(&pinned))
            .unwrap();

        assert!(matched.override_used());
        assert_eq!(matched.model(), &pinned);
        assert!(matched.fallbacks().is_empty());
    }

    #[test]
    fn should_match_every_condition_shape_when_all_present_conditions_hold() {
        // The four match-condition shapes of a rule, each with a task that
        // satisfies every present condition: none, intent, path, path+intent.
        let cases = [
            (
                json!({ "name": "none", "model": "ollama/m" }),
                TaskIntent::Chat,
                vec!["x.rs"],
            ),
            (
                json!({ "name": "intent", "intent": "edit", "model": "ollama/m" }),
                TaskIntent::Edit,
                vec!["x.rs"],
            ),
            (
                json!({ "name": "path", "paths": ["src/**"], "model": "ollama/m" }),
                TaskIntent::Chat,
                vec!["src/x.rs"],
            ),
            (
                json!({ "name": "both", "intent": "edit", "paths": ["src/**"], "model": "ollama/m" }),
                TaskIntent::Edit,
                vec!["src/x.rs"],
            ),
        ];

        for (rule, intent, paths) in cases {
            let name = rule["name"].as_str().unwrap().to_owned();
            let routing = policy(json!({
                "default": { "model": "openai/gpt-5.5-mini" },
                "rules": [rule],
            }));
            let task = task_with(intent, &paths);

            let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

            assert_eq!(
                matched.matched_rule().map(|rule| rule.name.as_str()),
                Some(name.as_str()),
                "rule '{name}' should match"
            );
        }
    }

    #[test]
    fn should_fall_through_to_default_when_a_present_condition_fails() {
        // Every way a single present condition can fail: a wrong intent, a
        // non-matching path, and each half of a path+intent rule missing while
        // the other holds (proving the conditions are AND-combined).
        let cases = [
            (
                json!({ "name": "intent", "intent": "edit", "model": "ollama/m" }),
                TaskIntent::Chat,
                vec!["x.rs"],
            ),
            (
                json!({ "name": "path", "paths": ["src/**"], "model": "ollama/m" }),
                TaskIntent::Chat,
                vec!["docs/x.md"],
            ),
            (
                json!({ "name": "intent ok, path miss", "intent": "edit", "paths": ["src/**"], "model": "ollama/m" }),
                TaskIntent::Edit,
                vec!["docs/x.md"],
            ),
            (
                json!({ "name": "path ok, intent miss", "intent": "edit", "paths": ["src/**"], "model": "ollama/m" }),
                TaskIntent::Chat,
                vec!["src/x.rs"],
            ),
        ];

        for (rule, intent, paths) in cases {
            let name = rule["name"].as_str().unwrap().to_owned();
            let routing = policy(json!({
                "default": { "model": "openai/gpt-5.5-mini" },
                "rules": [rule],
            }));
            let task = task_with(intent, &paths);

            let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

            assert!(
                matches!(matched.source, RouteSource::Default { .. }),
                "rule '{name}' should not match"
            );
        }
    }

    #[test]
    fn should_match_when_any_glob_in_the_paths_list_matches() {
        // The path condition is OR across its globs and across the candidate
        // paths: only the second glob matches, only the second file matches.
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{
                "name": "multi",
                "paths": ["src/auth/**", "src/crypto/**"],
                "model": "ollama/m",
            }],
        }));
        let task = task_with(TaskIntent::Chat, &["docs/readme.md", "src/crypto/aes.rs"]);

        let matched = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(matched.matched_rule().unwrap().name, "multi");
    }

    #[test]
    fn should_be_deterministic_across_calls() {
        let routing = policy(json!({
            "default": { "model": "openai/gpt-5.5-mini" },
            "rules": [{ "name": "edits", "intent": "edit", "model": "ollama/llama3" }],
        }));
        let task = task_with(TaskIntent::Edit, &["src/main.rs"]);

        let first = PolicyMatcher.match_policy(&task, &routing, None).unwrap();
        let second = PolicyMatcher.match_policy(&task, &routing, None).unwrap();

        assert_eq!(first, second);
    }
}

//! Deterministic routing rules and the routing policy.

use std::path::PathBuf;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::glob::compile_globs;
use crate::effort::Effort;
use crate::intent::TaskIntent;
use crate::model::{ModelCapabilities, ModelReference};
use crate::policy::ToolsConfig;

/// Default priority for a rule that does not set one.
///
/// Rules are evaluated in ascending priority order, so an unprioritized rule
/// evaluates after any rule with an explicit lower number.
const DEFAULT_PRIORITY: u32 = 1000;

/// Match specificity of a routing rule, used as a precedence tie-breaker.
///
/// Higher is more specific. The ladder is
/// `skill+path+intent > skill+path > path+intent > skill > path > intent > default`.
/// A rule combining `skill` with `intent` but no `path` collapses to
/// [`Skill`](Self::Skill); there is no dedicated skill+intent rung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Specificity {
    /// No match conditions.
    Default = 0,
    /// Intent only.
    Intent = 1,
    /// Path only.
    Path = 2,
    /// Skill (optionally with intent, no path).
    Skill = 3,
    /// Path and intent.
    PathIntent = 4,
    /// Skill and path.
    SkillPath = 5,
    /// Skill, path and intent.
    SkillPathIntent = 6,
}

/// A single deterministic routing rule.
///
/// A rule selects [`model`](Self::model) (with optional per-rule
/// [`fallbacks`](Self::fallbacks)) when all of its present match conditions
/// hold. Matching is LLM-free. The match conditions are flat and all optional:
///
/// - [`intent`](Self::intent): the classified task intent.
/// - [`skill`](Self::skill): the invoked skill name.
/// - [`paths`](Self::paths): file-path globs; a rule with paths matches when a
///   relevant path matches any glob.
///
/// A rule with no conditions matches everything (useful as a low-priority
/// catch-all). Rules are evaluated in ascending [`priority`](Self::priority)
/// order; the first match wins.
///
/// When [`local_only`](Self::local_only) is set, a matched rule must not fall
/// back to remote models — its fallback chain is restricted to local models.
///
/// A matched rule also declares the [`effort`](Self::effort) the model should
/// spend on the task, defaulting to [`Effort::Medium`].
///
/// # Examples
///
/// ```
/// use smista_core::effort::Effort;
/// use smista_core::policy::RoutingRule;
///
/// let rule: RoutingRule = serde_json::from_value(serde_json::json!({
///     "name": "auth code uses Claude",
///     "priority": 30,
///     "effort": "high",
///     "intent": "edit",
///     "paths": ["src/auth/**"],
///     "model": "anthropic/claude-sonnet",
///     "fallbacks": ["openai/gpt-5.5-thinking"],
/// }))
/// .unwrap();
/// assert_eq!(rule.name, "auth code uses Claude");
/// assert_eq!(rule.priority, 30);
/// assert_eq!(rule.effort, Effort::High);
/// assert_eq!(rule.model.model, "claude-sonnet");
/// assert_eq!(rule.paths, vec!["src/auth/**".to_string()]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Human-readable rule name.
    pub name: String,
    /// Evaluation priority; rules are evaluated in ascending order (lower
    /// first). Defaults to `1000` when unset.
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Reasoning effort the model should spend on a matched task. Defaults to
    /// [`Effort::Medium`] when unset.
    #[serde(default)]
    pub effort: Effort,
    /// Required task intent, if any.
    #[serde(default)]
    pub intent: Option<TaskIntent>,
    /// Required skill name, if any.
    #[serde(default)]
    pub skill: Option<String>,
    /// File-path globs; when non-empty, a relevant path must match one of them.
    #[serde(default)]
    pub paths: Vec<String>,
    /// When set, a matched rule must not fall back to remote models; its
    /// fallback chain is restricted to local models.
    #[serde(default)]
    pub local_only: bool,
    /// Capability gate: when set, the matched model must satisfy every
    /// capability flagged `true` here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_capabilities: Option<ModelCapabilities>,
    /// Model selected when the rule matches. Using syntax supported by [`ModelReference`] (e.g. `"ollama/llama3"`).
    pub model: ModelReference,
    /// Models tried, in order, when the selected model is unavailable. Subject
    /// to [`local_only`](Self::local_only).
    #[serde(default)]
    pub fallbacks: Vec<ModelReference>,
    /// Tool permissions the matched route requires.
    ///
    /// Merged over project defaults via [`ToolsConfig::narrow`].
    #[serde(default)]
    pub required_permissions: ToolsConfig,
    /// Per-task cost ceiling for the matched route, in account currency.
    ///
    /// Serialized as a string for exact precision.
    #[serde(
        default,
        with = "rust_decimal::serde::str_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub cost_limit: Option<Decimal>,
}

/// The observable inputs a [`RoutingRule`] is matched against.
///
/// Built by the router from the classified task. Matching is LLM-free.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoutingContext {
    /// The classified task intent, if known.
    pub intent: Option<TaskIntent>,
    /// The invoked skill name, if any.
    pub skill: Option<String>,
    /// Candidate file paths relevant to the task.
    pub paths: Vec<PathBuf>,
}

impl RoutingRule {
    /// Returns `true` when every present condition holds for `ctx`.
    ///
    /// Conditions across fields are AND-combined; lists within a field (paths)
    /// are OR-combined; absent conditions are ignored. A rule with no
    /// conditions matches everything. Invalid path globs match nothing here
    /// rather than panicking.
    #[must_use]
    pub fn matches(&self, ctx: &RoutingContext) -> bool {
        if let Some(intent) = self.intent
            && ctx.intent != Some(intent)
        {
            tracing::debug!(
                rule.name = %self.name,
                rule.intent = %intent,
                "rule {{rule.name}} does not match: intent mismatch"
            );
            return false;
        }
        if let Some(skill) = &self.skill
            && ctx.skill.as_deref() != Some(skill.as_str())
        {
            tracing::debug!(
                rule.name = %self.name,
                rule.skill = %skill,
                "rule {{rule.name}} does not match: skill mismatch"
            );
            return false;
        }
        if !self.paths.is_empty() {
            let set = compile_globs(&self.paths).unwrap_or_else(|source| {
                tracing::warn!(
                    rule.name = %self.name,
                    error.message = %source,
                    "rule {{rule.name}} has invalid path globs; treating as no match"
                );
                globset::GlobSet::empty()
            });
            tracing::trace!(
                rule.name = %self.name,
                rule.paths = self.paths.len(),
                "matching {{rule.paths}} path globs for rule {{rule.name}}"
            );
            if !ctx.paths.iter().any(|path| set.is_match(path)) {
                tracing::debug!(
                    rule.name = %self.name,
                    "rule {{rule.name}} does not match: no path matched"
                );
                return false;
            }
        }
        tracing::debug!(rule.name = %self.name, "rule {{rule.name}} matches context");
        true
    }

    /// Orders two rules by routing precedence: lower [`priority`](Self::priority)
    /// first, then higher [`Specificity`](Self::specificity) first.
    ///
    /// Returns [`core::cmp::Ordering::Equal`] when both priority and specificity
    /// match; configuration validation rejects that case rather than silently
    /// tie-breaking. Callers preserve config order with a stable sort.
    #[must_use]
    pub fn precedence_cmp(a: &Self, b: &Self) -> core::cmp::Ordering {
        a.priority
            .cmp(&b.priority)
            .then_with(|| b.specificity().cmp(&a.specificity()))
    }

    /// Returns the [`Specificity`] of this rule from its present conditions.
    #[must_use]
    pub fn specificity(&self) -> Specificity {
        let intent = self.intent.is_some();
        let path = !self.paths.is_empty();
        let skill = self.skill.is_some();
        match (skill, path, intent) {
            (true, true, true) => Specificity::SkillPathIntent,
            (true, true, false) => Specificity::SkillPath,
            (false, true, true) => Specificity::PathIntent,
            (true, false, _) => Specificity::Skill,
            (false, true, false) => Specificity::Path,
            (false, false, true) => Specificity::Intent,
            (false, false, false) => Specificity::Default,
        }
    }
}

/// Returns the default rule priority.
fn default_priority() -> u32 {
    DEFAULT_PRIORITY
}

/// The routing policy: a set of rules plus a default route.
///
/// Rules are evaluated in ascending priority order; the first match wins. When
/// no rule matches, the [`default`](Self::default) route is used. A complete
/// policy must define a default route; its absence is a validation error,
/// so it is modelled as optional here.
///
/// # Examples
///
/// ```
/// use smista_core::policy::RoutingPolicy;
///
/// let policy = RoutingPolicy::default();
/// assert!(policy.rules.is_empty());
/// assert!(policy.default.is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoutingPolicy {
    /// Routing rules; evaluated in ascending priority order, first match wins.
    #[serde(default)]
    pub rules: Vec<RoutingRule>,
    /// Route used when no rule matches.
    #[serde(default)]
    pub default: Option<DefaultRoute>,
}

/// The default route, applied when no routing rule matches.
///
/// Mirrors the `[routing.default]` table.
///
/// # Examples
///
/// ```
/// use smista_core::policy::DefaultRoute;
///
/// let route: DefaultRoute = serde_json::from_value(serde_json::json!({
///     "model": "openai/gpt-5.5-mini",
///     "fallbacks": ["ollama/qwen2.5-coder"],
/// }))
/// .unwrap();
/// assert_eq!(route.model.to_string(), "openai/gpt-5.5-mini");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultRoute {
    /// Model used when no rule matches.
    pub model: ModelReference,
    /// Models tried, in order, when the default model is unavailable.
    #[serde(default)]
    pub fallbacks: Vec<ModelReference>,
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::path::PathBuf;
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::*;
    use crate::policy::PermissionMode;

    fn rule_with(intent: bool, skill: bool, path: bool) -> RoutingRule {
        let mut rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "t",
            "model": "ollama/llama3",
        }))
        .unwrap();
        if intent {
            rule.intent = Some(TaskIntent::Edit);
        }
        if skill {
            rule.skill = Some("changelog".to_string());
        }
        if path {
            rule.paths = vec!["src/**".to_string()];
        }
        rule
    }

    #[test]
    fn should_default_to_empty_policy() {
        let policy = RoutingPolicy::default();
        assert!(policy.rules.is_empty());
        assert!(policy.default.is_none());
    }

    #[test]
    fn should_default_priority_when_unset() {
        let rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "catch all",
            "model": "ollama/llama3",
        }))
        .unwrap();
        assert_eq!(rule.priority, DEFAULT_PRIORITY);
        assert_eq!(rule.effort, Effort::Medium);
        assert!(rule.intent.is_none());
        assert!(rule.skill.is_none());
        assert!(rule.paths.is_empty());
        assert!(!rule.local_only);
        assert!(rule.requires_capabilities.is_none());
        assert!(rule.fallbacks.is_empty());
        assert!(rule.required_permissions.permissions.is_empty());
        assert!(rule.cost_limit.is_none());
    }

    #[test]
    fn should_parse_skill_match_with_per_rule_fallbacks() {
        let rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "use local model for changelog skill",
            "priority": 20,
            "skill": "changelog",
            "model": "ollama/qwen2.5-coder",
            "fallbacks": ["openai/gpt-5.5-mini"],
        }))
        .unwrap();
        assert_eq!(rule.skill.as_deref(), Some("changelog"));
        assert_eq!(rule.model.to_string(), "ollama/qwen2.5-coder");
        assert_eq!(rule.fallbacks.len(), 1);
        assert_eq!(rule.fallbacks[0].to_string(), "openai/gpt-5.5-mini");
    }

    #[test]
    fn should_parse_combined_conditions_with_local_only() {
        let rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "review security-sensitive code locally",
            "priority": 5,
            "effort": "low",
            "intent": "review",
            "paths": ["src/crypto/**", "src/auth/**"],
            "local_only": true,
            "model": "ollama/qwen2.5-coder",
        }))
        .unwrap();
        assert_eq!(rule.priority, 5);
        assert_eq!(rule.effort, Effort::Low);
        assert_eq!(rule.intent, Some(TaskIntent::Review));
        assert_eq!(
            rule.paths,
            vec!["src/crypto/**".to_string(), "src/auth/**".to_string()]
        );
        assert!(rule.local_only);
    }

    #[test]
    fn should_parse_full_policy_with_default_route() {
        let policy: RoutingPolicy = serde_json::from_value(serde_json::json!({
            "default": {
                "model": "openai/gpt-5.5-mini",
                "fallbacks": ["ollama/qwen2.5-coder"],
            },
            "rules": [{
                "name": "plan with strongest reasoning model",
                "priority": 10,
                "intent": "plan",
                "model": "openai/gpt-5.5-thinking",
                "fallbacks": ["anthropic/claude-sonnet"],
            }],
        }))
        .unwrap();
        let default = policy.default.unwrap();
        assert_eq!(default.model.to_string(), "openai/gpt-5.5-mini");
        assert_eq!(default.fallbacks.len(), 1);
        assert_eq!(policy.rules.len(), 1);
        assert_eq!(policy.rules[0].intent, Some(TaskIntent::Plan));
    }

    #[test]
    fn should_roundtrip_serde() {
        let policy = RoutingPolicy {
            rules: vec![RoutingRule {
                name: "auth code uses Claude".to_string(),
                priority: 30,
                effort: Effort::High,
                intent: Some(TaskIntent::Edit),
                skill: None,
                paths: vec!["src/auth/**".to_string()],
                local_only: false,
                requires_capabilities: None,
                model: "anthropic/claude-sonnet".parse().unwrap(),
                fallbacks: vec!["openai/gpt-5.5-thinking".parse().unwrap()],
                required_permissions: ToolsConfig::default(),
                cost_limit: None,
            }],
            default: Some(DefaultRoute {
                model: "openai/gpt-5.5-mini".parse().unwrap(),
                fallbacks: vec!["ollama/qwen2.5-coder".parse().unwrap()],
            }),
        };
        let json = serde_json::to_string(&policy).unwrap();
        assert_eq!(
            serde_json::from_str::<RoutingPolicy>(&json).unwrap(),
            policy
        );
    }

    #[test]
    fn should_match_empty_rule_against_anything() {
        let rule = rule_with(false, false, false);
        let ctx = RoutingContext {
            intent: Some(TaskIntent::Chat),
            skill: Some("x".to_string()),
            paths: vec![PathBuf::from("a/b")],
        };
        assert!(rule.matches(&ctx));
    }

    #[test]
    fn should_match_when_all_present_conditions_hold() {
        let mut rule = rule_with(true, false, true);
        rule.skill = None;

        let hit = RoutingContext {
            intent: Some(TaskIntent::Edit),
            skill: None,
            paths: vec![PathBuf::from("src/auth/login.rs")],
        };
        assert!(rule.matches(&hit));

        let wrong_intent = RoutingContext {
            intent: Some(TaskIntent::Review),
            skill: None,
            paths: vec![PathBuf::from("src/auth/login.rs")],
        };
        assert!(!rule.matches(&wrong_intent));

        let no_path = RoutingContext {
            intent: Some(TaskIntent::Edit),
            skill: None,
            paths: vec![PathBuf::from("docs/readme.md")],
        };
        assert!(!rule.matches(&no_path));
    }

    #[test]
    fn should_order_by_priority_then_specificity() {
        let mut low_priority = rule_with(true, true, true);
        low_priority.priority = 100;
        let mut high_priority = rule_with(true, false, false);
        high_priority.priority = 10;

        assert_eq!(
            RoutingRule::precedence_cmp(&high_priority, &low_priority),
            Ordering::Less
        );

        let mut a = rule_with(true, true, true);
        a.priority = 10;
        let mut b = rule_with(true, false, false);
        b.priority = 10;
        assert_eq!(RoutingRule::precedence_cmp(&a, &b), Ordering::Less);

        let c = a.clone();
        assert_eq!(RoutingRule::precedence_cmp(&a, &c), Ordering::Equal);
    }

    #[test]
    fn should_parse_capabilities_permissions_and_cost_limit() {
        let rule: RoutingRule = serde_json::from_value(serde_json::json!({
            "name": "expensive remote review",
            "priority": 5,
            "intent": "review",
            "requires_capabilities": { "reasoning": true, "tools": true },
            "required_permissions": { "permissions": { "shell": "deny", "network": "ask" } },
            "cost_limit": "0.50",
            "model": "anthropic/claude-sonnet",
        }))
        .unwrap();

        let caps = rule.requires_capabilities.expect("capabilities present");
        assert!(caps.reasoning);
        assert!(caps.tools);
        assert_eq!(
            rule.required_permissions.mode_for("shell"),
            Some(PermissionMode::Deny)
        );
        assert_eq!(rule.cost_limit, Some(Decimal::from_str("0.50").unwrap()));
    }

    #[test]
    fn should_rank_specificity_by_ladder() {
        assert_eq!(
            rule_with(false, false, false).specificity(),
            Specificity::Default
        );
        assert_eq!(
            rule_with(true, false, false).specificity(),
            Specificity::Intent
        );
        assert_eq!(
            rule_with(false, false, true).specificity(),
            Specificity::Path
        );
        assert_eq!(
            rule_with(false, true, false).specificity(),
            Specificity::Skill
        );
        assert_eq!(
            rule_with(true, false, true).specificity(),
            Specificity::PathIntent
        );
        assert_eq!(
            rule_with(false, true, true).specificity(),
            Specificity::SkillPath
        );
        assert_eq!(
            rule_with(true, true, true).specificity(),
            Specificity::SkillPathIntent
        );
        assert_eq!(
            rule_with(true, true, false).specificity(),
            Specificity::Skill
        );
        assert!(Specificity::SkillPathIntent > Specificity::SkillPath);
        assert!(Specificity::PathIntent > Specificity::Skill);
        assert!(Specificity::Path > Specificity::Intent);
    }
}

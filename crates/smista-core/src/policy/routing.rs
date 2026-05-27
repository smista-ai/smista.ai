//! Deterministic routing rules and the routing policy.

use serde::{Deserialize, Serialize};

use crate::effort::Effort;
use crate::intent::TaskIntent;
use crate::model::ModelReference;

/// Default priority for a rule that does not set one.
///
/// Rules are evaluated in ascending priority order, so an unprioritized rule
/// evaluates after any rule with an explicit lower number.
const DEFAULT_PRIORITY: u32 = 1000;

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
    /// Model selected when the rule matches. Using syntax supported by [`ModelReference`] (e.g. `"ollama/llama3"`).
    pub model: ModelReference,
    /// Models tried, in order, when the selected model is unavailable. Subject
    /// to [`local_only`](Self::local_only).
    #[serde(default)]
    pub fallbacks: Vec<ModelReference>,
}

/// Returns the default rule priority.
fn default_priority() -> u32 {
    DEFAULT_PRIORITY
}

/// The routing policy: a set of rules plus a default route.
///
/// Rules are evaluated in ascending priority order; the first match wins. When
/// no rule matches, the [`default`](Self::default) route is used. A complete
/// policy must define a default route; its absence is a validation error (#4),
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
    use super::*;

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
        assert!(rule.fallbacks.is_empty());
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
                model: "anthropic/claude-sonnet".parse().unwrap(),
                fallbacks: vec!["openai/gpt-5.5-thinking".parse().unwrap()],
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
}

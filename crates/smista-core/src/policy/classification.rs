//! Deterministic task-classification configuration.

use serde::{Deserialize, Serialize};

use crate::intent::TaskIntent;

/// Default priority for a classification rule that does not set one.
///
/// Rules are evaluated in ascending priority order, so an unprioritized rule
/// evaluates after any rule with an explicit lower number.
const DEFAULT_PRIORITY: u32 = 1000;

/// Configuration for deterministic task classification.
///
/// Classification never depends on an LLM. Ordered [`rules`](Self::rules) map
/// observable signals — prompt keywords and required context — to a
/// [`TaskIntent`]. [`default_intent`](Self::default_intent) is used when no rule
/// matches.
///
/// # Examples
///
/// ```
/// use smista_core::intent::TaskIntent;
/// use smista_core::policy::ClassificationConfig;
///
/// let config = ClassificationConfig::default();
/// assert_eq!(config.default_intent, TaskIntent::Chat);
/// assert!(config.rules.is_empty());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassificationConfig {
    /// Intent used when no rule matches.
    #[serde(default = "default_intent")]
    pub default_intent: TaskIntent,
    /// Ordered classification rules.
    #[serde(default)]
    pub rules: Vec<ClassificationRule>,
}

/// A single deterministic classification rule.
///
/// All present conditions must hold for the rule to match. Conditions are
/// [`keywords`](Self::keywords) (any keyword present in the prompt) and
/// [`requires_any_context`](Self::requires_any_context) (at least one of the
/// named context kinds is available, e.g. `git_diff`, `pull_request`).
///
/// # Examples
///
/// ```
/// use smista_core::intent::TaskIntent;
/// use smista_core::policy::ClassificationRule;
///
/// let rule: ClassificationRule = serde_json::from_value(serde_json::json!({
///     "intent": "review",
///     "priority": 10,
///     "keywords": ["review", "audit", "check"],
///     "requires_any_context": ["git_diff", "pull_request"],
/// }))
/// .unwrap();
/// assert_eq!(rule.intent, TaskIntent::Review);
/// assert_eq!(rule.priority, 10);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassificationRule {
    /// Intent assigned when the rule matches.
    pub intent: TaskIntent,
    /// Evaluation priority; rules are evaluated in ascending order (lower
    /// first). Defaults to `1000` when unset.
    #[serde(default = "default_priority")]
    pub priority: u32,
    /// Keywords; the rule matches when any appears in the prompt.
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Required context kinds; the rule matches when any is available.
    #[serde(default)]
    pub requires_any_context: Vec<String>,
}

/// The default intent when no classification rule matches.
fn default_intent() -> TaskIntent {
    TaskIntent::Chat
}

/// Returns the default classification-rule priority.
fn default_priority() -> u32 {
    DEFAULT_PRIORITY
}

impl Default for ClassificationConfig {
    fn default() -> Self {
        Self {
            default_intent: default_intent(),
            rules: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_intent_to_chat() {
        assert_eq!(
            ClassificationConfig::default().default_intent,
            TaskIntent::Chat
        );
    }

    #[test]
    fn should_deserialize_from_empty_table() {
        let config: ClassificationConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, ClassificationConfig::default());
    }

    #[test]
    fn should_default_rule_priority_when_unset() {
        let rule: ClassificationRule = serde_json::from_value(serde_json::json!({
            "intent": "edit",
            "keywords": ["fix"],
        }))
        .unwrap();
        assert_eq!(rule.priority, DEFAULT_PRIORITY);
        assert!(rule.requires_any_context.is_empty());
    }

    #[test]
    fn should_roundtrip_serde_with_rules() {
        let config: ClassificationConfig = serde_json::from_value(serde_json::json!({
            "default_intent": "chat",
            "rules": [{
                "intent": "review",
                "priority": 10,
                "keywords": ["review", "audit"],
                "requires_any_context": ["git_diff"],
            }],
        }))
        .unwrap();
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(
            serde_json::from_str::<ClassificationConfig>(&json).unwrap(),
            config
        );
    }
}

//! Intent classification: deterministically maps a raw prompt to a [`TaskIntent`].
//!
//! Intent classification is the first stage of the routing pipeline. Given the
//! user's prompt plus the kinds of context available for the task (for example
//! `git_diff` or `pull_request`), an ordered list of [`ClassificationRule`]s is
//! evaluated to produce a [`TaskIntent`]. The first matching rule wins; if
//! none matches, [`ClassificationConfig::default_intent`] is used.
//!
//! Classification is purely deterministic — it never calls an LLM. The rules
//! are authored in the CLI configuration (`[classification]` in `config.toml`)
//! and sent to the router, which evaluates them: the router classifies on each
//! turn to produce a [`TaskIntent`], then a separate set of
//! [`RoutingRule`](super::RoutingRule)s selects the model.
//!
//! This module only defines the configuration and result shapes; the
//! classifier implementation lives elsewhere.

use serde::{Deserialize, Serialize};

use crate::intent::TaskIntent;

/// Default priority for a rule that does not set one.
///
/// Rules are evaluated in ascending priority order, so a rule without an
/// explicit priority evaluates after any rule with a lower number.
const DEFAULT_PRIORITY: u32 = 1000;

/// Configuration for intent classification.
///
/// Holds the ordered set of [`rules`](Self::rules) used to infer a
/// [`TaskIntent`] from a prompt and its available context kinds, plus the
/// [`default_intent`](Self::default_intent) returned when no rule matches.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct ClassificationConfig {
    /// Intent used when no rule matches.
    #[serde(default = "default_intent")]
    pub default_intent: TaskIntent,
    /// Ordered intent-classification rules.
    #[serde(default)]
    pub rules: Vec<ClassificationRule>,
}

/// A single intent-classification rule.
///
/// A rule maps observable signals in the request to a [`TaskIntent`]. All
/// present conditions must hold for the rule to match:
///
/// - [`keywords`](Self::keywords) — at least one keyword appears in the prompt.
/// - [`requires_any_context`](Self::requires_any_context) — at least one of the
///   named context kinds (for example `git_diff` or `pull_request`) is
///   available for the task.
///
/// Absent conditions are ignored; a rule with no conditions matches every
/// request.
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
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

/// The outcome of intent classification for one request.
///
/// Either the user supplied an explicit intent (see [`IntentSource::Explicit`])
/// or one was inferred deterministically from a matching
/// [`ClassificationRule`] (see [`IntentSource::Inferred`]). For an inferred
/// intent, [`matched_rule`](Self::matched_rule) carries the index of the
/// matching entry in [`ClassificationConfig::rules`] so traces can point back
/// to the configured rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Classification {
    /// The detected task intent.
    pub intent: TaskIntent,
    /// Whether the intent was given explicitly by the user or inferred.
    pub source: IntentSource,
    /// Human-readable explanation of why this intent was chosen.
    pub reason: String,
    /// Index of the matched rule in [`ClassificationConfig::rules`], if any.
    ///
    /// Refers to a [`ClassificationRule`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub matched_rule: Option<usize>,
    /// Optional deterministic confidence category for an inferred intent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub confidence: Option<Confidence>,
}

/// Deterministic confidence category for an inferred intent.
///
/// This is a coarse signal-strength label, not a probability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Confidence {
    /// Weak signal.
    Low,
    /// Moderate signal.
    Medium,
    /// Strong signal.
    High,
}

/// Origin of the intent attached to a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum IntentSource {
    /// The user named the intent (for example via `--intent`).
    Explicit,
    /// The intent was inferred from a matching [`ClassificationRule`].
    Inferred,
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
    fn should_build_inferred_classification_result() {
        let result = Classification {
            intent: TaskIntent::Review,
            source: IntentSource::Inferred,
            reason: "keyword 'review' matched rule 0".to_string(),
            matched_rule: Some(0),
            confidence: Some(Confidence::High),
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["source"], "inferred");
        assert_eq!(json["confidence"], "high");
        assert_eq!(
            serde_json::from_value::<Classification>(json).unwrap(),
            result
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

    #[test]
    fn should_omit_optional_fields_when_absent() {
        let result = Classification {
            intent: TaskIntent::Chat,
            source: IntentSource::Explicit,
            reason: "explicit --intent flag".to_string(),
            matched_rule: None,
            confidence: None,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert!(json.get("matched_rule").is_none());
        assert!(json.get("confidence").is_none());
    }
}

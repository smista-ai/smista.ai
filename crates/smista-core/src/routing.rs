//! Deterministic routing decisions shared across router and clients.
//!
//! [`RoutingDecision`] records the complete explanation of one model-selection
//! result: the intent, selected provider and model, matched rule, selection
//! modifiers, and the human-readable reason produced by the deterministic
//! router.

use serde::{Deserialize, Serialize};

use crate::intent::TaskIntent;
use crate::model::Provider;

/// The complete deterministic routing decision for one task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct RoutingDecision {
    /// Task intent that drove routing.
    pub intent: TaskIntent,
    /// Provider selected to serve the task.
    pub provider: Provider,
    /// Model selected to serve the task.
    pub model: String,
    /// Name of the routing rule that matched, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub matched_rule: Option<String>,
    /// Whether a fallback model, rather than the primary, was selected.
    pub fallback_used: bool,
    /// Whether an explicit model override was selected.
    pub override_used: bool,
    /// Human-readable explanation of why the route was selected.
    pub reason: String,
}

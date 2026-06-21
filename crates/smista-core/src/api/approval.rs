//! The decision made on an approval.
//!
//! [`ApprovalDecision`] is `approved` or `rejected`. There is no standalone
//! approval endpoint: a tool's approval rides the decision on its tool result,
//! and an approval with no tool to run is decided in the `/continue` bundle. See
//! [`ContinueRequest`](super::ContinueRequest).
//!
//! # Examples
//!
//! ```
//! use smista_core::api::ApprovalDecision;
//!
//! let json = serde_json::to_string(&ApprovalDecision::Approved).unwrap();
//! assert_eq!(json, "\"approved\"");
//! ```

use serde::{Deserialize, Serialize};
#[cfg(feature = "surrealdb")]
use surrealdb_types::SurrealValue;

/// The decision made on a pending approval.
///
/// Each variant serializes to its lowercase name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "surrealdb", derive(SurrealValue))]
#[cfg_attr(feature = "surrealdb", surreal(crate = "::surrealdb_types"))]
#[cfg_attr(feature = "surrealdb", surreal(rename_all = "lowercase", untagged))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ApprovalDecision {
    /// The action is permitted to proceed.
    Approved,
    /// The action is denied.
    Rejected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_decision_as_lowercase() {
        assert_eq!(
            serde_json::to_value(ApprovalDecision::Rejected).unwrap(),
            serde_json::json!("rejected")
        );
    }

    #[test]
    fn should_roundtrip_every_variant() {
        for decision in [ApprovalDecision::Approved, ApprovalDecision::Rejected] {
            let json = serde_json::to_string(&decision).unwrap();
            assert_eq!(
                serde_json::from_str::<ApprovalDecision>(&json).unwrap(),
                decision
            );
        }
    }
}

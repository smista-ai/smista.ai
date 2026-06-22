//! Tool-call domain types shared across the workspace.
//!
//! These types describe a tool invocation independently of where it is stored or
//! reported. [`ToolCallStatus`] is the lifecycle status persisted by the storage
//! `session_tool_call` entity and recorded by the router in a tool-call trace
//! event, so it lives here as the single source of truth rather than in either
//! consumer.

use serde::{Deserialize, Serialize};
#[cfg(feature = "surrealdb")]
use surrealdb_types::SurrealValue;

/// Execution status of a tool call.
///
/// **Provisional**: these variants are a placeholder until the private spec
/// pins the tool-call lifecycle. Serialized as `snake_case`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "surrealdb", derive(SurrealValue))]
#[cfg_attr(feature = "surrealdb", surreal(crate = "::surrealdb_types"))]
#[cfg_attr(feature = "surrealdb", surreal(rename_all = "snake_case", untagged))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(export))]
pub enum ToolCallStatus {
    /// Requested, not yet started.
    Pending,
    /// Currently executing.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_status_as_snake_case() {
        assert_eq!(
            serde_json::to_value(ToolCallStatus::Completed).unwrap(),
            serde_json::Value::String("completed".to_string())
        );
    }

    #[test]
    fn should_roundtrip_status() {
        for status in [
            ToolCallStatus::Pending,
            ToolCallStatus::Running,
            ToolCallStatus::Completed,
            ToolCallStatus::Failed,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(
                serde_json::from_str::<ToolCallStatus>(&json).unwrap(),
                status
            );
        }
    }
}

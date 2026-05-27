//! Response body for previewing how a task would be routed.
//!
//! [`PreviewResponse`] answers `POST /sessions/{id}/preview`: which model the
//! deterministic router would select, what context it would include or exclude,
//! the estimated cost range, and the permissions the task would require. The
//! preview never calls the selected model.
//!
//! The request body is identical to
//! [`ExecuteRequest`](crate::api::ExecuteRequest), so no preview-specific
//! request type exists.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::CostRange;
//!
//! let cost = CostRange { min: 0.03, max: 0.09, currency: "USD".to_string() };
//! let json = serde_json::to_string(&cost).unwrap();
//! assert!(json.contains("\"currency\":\"USD\""));
//! ```

use serde::{Deserialize, Serialize};

use crate::intent::TaskIntent;
use crate::model::Provider;
use crate::policy::PermissionMode;

/// The predicted routing of a task, without calling the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewResponse {
    /// Task type that would be detected.
    pub task_type: TaskIntent,
    /// Provider that would serve the task.
    pub provider: Provider,
    /// Model that would serve the task.
    pub model: String,
    /// Description of the routing rule that would match, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<String>,
    /// Human-readable descriptions of context that would be included.
    pub included_context: Vec<String>,
    /// Human-readable descriptions of context that would be excluded.
    pub excluded_context: Vec<String>,
    /// Estimated cost range for the task.
    pub estimated_cost: CostRange,
    /// Permissions the task would require.
    pub required_permissions: Vec<RequiredPermission>,
}

/// An estimated cost range in a given currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostRange {
    /// Lower bound of the estimate.
    pub min: f64,
    /// Upper bound of the estimate.
    pub max: f64,
    /// ISO 4217 currency code for the bounds.
    pub currency: String,
}

/// A permission the task requires and the mode that would apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredPermission {
    /// Name of the permission, for example `write_files`.
    pub permission: String,
    /// Mode that would apply to the permission.
    pub mode: PermissionMode,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_spec_preview_response() {
        let json = r#"{
            "task_type": "review",
            "provider": "openai",
            "model": "gpt-5.5-thinking",
            "matched_rule": "task.review -> openai/gpt-5.5-thinking",
            "included_context": ["current git diff", "SMISTA.md"],
            "excluded_context": [".env", "target/**"],
            "estimated_cost": { "min": 0.03, "max": 0.09, "currency": "USD" },
            "required_permissions": [
                { "permission": "read_repository", "mode": "allow" },
                { "permission": "write_files", "mode": "ask" }
            ]
        }"#;
        let preview: PreviewResponse = serde_json::from_str(json).unwrap();

        assert_eq!(preview.task_type, TaskIntent::Review);
        assert_eq!(preview.provider, Provider::OpenAI);
        assert_eq!(preview.estimated_cost.max, 0.09);
        assert_eq!(
            preview.required_permissions[1],
            RequiredPermission {
                permission: "write_files".to_string(),
                mode: PermissionMode::Ask,
            }
        );
    }

    #[test]
    fn should_roundtrip_preview_response() {
        let preview = PreviewResponse {
            task_type: TaskIntent::Review,
            provider: Provider::OpenAI,
            model: "gpt-5.5-thinking".to_string(),
            matched_rule: None,
            included_context: vec!["SMISTA.md".to_string()],
            excluded_context: Vec::new(),
            estimated_cost: CostRange {
                min: 0.01,
                max: 0.05,
                currency: "USD".to_string(),
            },
            required_permissions: Vec::new(),
        };
        let json = serde_json::to_string(&preview).unwrap();
        assert_eq!(
            serde_json::from_str::<PreviewResponse>(&json).unwrap(),
            preview
        );
    }
}

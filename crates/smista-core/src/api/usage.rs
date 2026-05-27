//! Response body for a session's token usage and cost breakdown.
//!
//! [`SessionUsageResponse`] answers `GET /sessions/{id}/usage`: the session's
//! total usage plus per-model and per-task-type breakdowns. The shared token
//! and cost fields reuse [`Usage`](crate::usage::Usage), flattened into each
//! breakdown entry alongside its `request_count`.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::ModelUsage;
//! use smista_core::model::Provider;
//! use smista_core::usage::Usage;
//!
//! let entry = ModelUsage {
//!     provider: Provider::OpenAI,
//!     model: "gpt-5.5-thinking".to_string(),
//!     usage: Usage { input_tokens: Some(8_000), ..Default::default() },
//!     request_count: 3,
//! };
//! let value = serde_json::to_value(&entry).unwrap();
//! // `Usage` fields are flattened alongside `request_count`.
//! assert_eq!(value["input_tokens"], 8_000);
//! assert_eq!(value["request_count"], 3);
//! ```

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::intent::TaskIntent;
use crate::model::Provider;
use crate::usage::Usage;

/// Response to `GET /sessions/{id}/usage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUsageResponse {
    /// Session the usage belongs to.
    pub session_id: Uuid,
    /// Usage totals and breakdowns.
    pub usage: UsageBreakdown,
}

/// A session's usage total and its per-model and per-task-type breakdowns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageBreakdown {
    /// Combined usage across the whole session.
    pub total: Usage,
    /// Usage attributed to each model.
    pub by_model: Vec<ModelUsage>,
    /// Usage attributed to each task type.
    pub by_task_type: Vec<TaskTypeUsage>,
}

/// Usage attributed to a single model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    /// Provider that served the requests.
    pub provider: Provider,
    /// Model that served the requests.
    pub model: String,
    /// Token and cost totals for the model.
    #[serde(flatten)]
    pub usage: Usage,
    /// Number of requests served by the model.
    pub request_count: u32,
}

/// Usage attributed to a single task type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskTypeUsage {
    /// Task type the requests were classified as.
    pub task_type: TaskIntent,
    /// Token and cost totals for the task type.
    #[serde(flatten)]
    pub usage: Usage,
    /// Number of requests of this task type.
    pub request_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_spec_usage() {
        let json = r#"{
            "session_id": "00000000-0000-0000-0000-000000000000",
            "usage": {
                "total": {
                    "input_tokens": 12000,
                    "output_tokens": 4200,
                    "total_tokens": 16200,
                    "estimated_cost": 0.42,
                    "currency": "USD"
                },
                "by_model": [
                    {
                        "provider": "openai",
                        "model": "gpt-5.5-thinking",
                        "input_tokens": 8000,
                        "output_tokens": 2200,
                        "total_tokens": 10200,
                        "estimated_cost": 0.31,
                        "currency": "USD",
                        "request_count": 3
                    }
                ],
                "by_task_type": [
                    {
                        "task_type": "plan",
                        "input_tokens": 4000,
                        "output_tokens": 1200,
                        "estimated_cost": 0.18,
                        "request_count": 1
                    }
                ]
            }
        }"#;
        let response: SessionUsageResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.session_id, Uuid::nil());
        assert_eq!(response.usage.total.total_tokens, Some(16_200));
        assert_eq!(response.usage.by_model[0].request_count, 3);
        assert_eq!(response.usage.by_model[0].usage.input_tokens, Some(8_000));
        assert_eq!(response.usage.by_task_type[0].task_type, TaskIntent::Plan);
        assert_eq!(
            response.usage.by_task_type[0].usage.estimated_cost,
            Some(0.18)
        );
    }

    #[test]
    fn should_flatten_usage_into_model_entry() {
        let entry = ModelUsage {
            provider: Provider::OpenAI,
            model: "gpt-5.5-thinking".to_string(),
            usage: Usage {
                input_tokens: Some(8_000),
                ..Default::default()
            },
            request_count: 3,
        };
        let value = serde_json::to_value(&entry).unwrap();
        assert_eq!(value["provider"], "openai");
        assert_eq!(value["input_tokens"], 8_000);
        assert_eq!(value["request_count"], 3);
    }

    #[test]
    fn should_roundtrip_task_type_usage() {
        let entry = TaskTypeUsage {
            task_type: TaskIntent::Plan,
            usage: Usage {
                input_tokens: Some(4_000),
                ..Default::default()
            },
            request_count: 1,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert_eq!(serde_json::from_str::<TaskTypeUsage>(&json).unwrap(), entry);
    }
}

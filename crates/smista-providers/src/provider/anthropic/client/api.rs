//! Anthropic API types for the `/v1/models` endpoint.

use serde::{Deserialize, Serialize};

/// One page of the `/v1/models` listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsResponse {
    /// The models on this page.
    pub data: Vec<Model>,
    /// Whether further pages follow this one.
    #[serde(default)]
    pub has_more: bool,
    /// The id of the last model on this page, used as the pagination cursor.
    #[serde(default)]
    pub last_id: Option<String>,
}

/// A single model entry from the `/v1/models` listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    /// The model identifier, e.g. `claude-opus-4-8`.
    pub id: String,
    /// A human-friendly name, when the API provides one.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Maximum number of input (context) tokens the model accepts.
    pub max_input_tokens: u32,
    /// Maximum number of output tokens the model emits, when bounded.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// The capabilities the model reports.
    #[serde(default)]
    pub capabilities: Capabilities,
}

/// The capability sub-objects the adapter reads from a model entry.
///
/// Each sub-object the API omits defaults to unsupported, so a model that does
/// not report a capability is treated as lacking it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Capabilities {
    /// Whether the model accepts image inputs.
    #[serde(default)]
    pub image_input: Supported,
    /// Whether the model can be constrained to structured (JSON) output.
    #[serde(default)]
    pub structured_outputs: Supported,
    /// Whether the model performs explicit reasoning (thinking).
    #[serde(default)]
    pub thinking: Supported,
}

/// A capability flag of the shape `{ "supported": bool }`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Supported {
    /// Whether the capability is supported. Defaults to `false` when absent.
    #[serde(default)]
    pub supported: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_a_model_entry_with_capabilities() {
        // Trimmed from the issue's `/v1/models` sample: the fields the adapter reads.
        let payload = r#"{
            "data": [
                {
                    "type": "model",
                    "id": "claude-opus-4-8",
                    "display_name": "Claude Opus 4.8",
                    "max_input_tokens": 1000000,
                    "max_tokens": 128000,
                    "capabilities": {
                        "image_input": { "supported": true },
                        "structured_outputs": { "supported": true },
                        "thinking": { "supported": true }
                    }
                }
            ],
            "has_more": false,
            "last_id": "claude-opus-4-8"
        }"#;

        let response: ModelsResponse = serde_json::from_str(payload).expect("parses");

        assert_eq!(response.data.len(), 1);
        let model = &response.data[0];
        assert_eq!(model.id, "claude-opus-4-8");
        assert_eq!(model.display_name.as_deref(), Some("Claude Opus 4.8"));
        assert_eq!(model.max_input_tokens, 1_000_000);
        assert_eq!(model.max_tokens, Some(128_000));
        assert!(model.capabilities.image_input.supported);
        assert!(model.capabilities.structured_outputs.supported);
        assert!(model.capabilities.thinking.supported);
        assert!(!response.has_more);
    }

    #[test]
    fn should_default_missing_capability_sub_objects_to_unsupported() {
        let payload = r#"{
            "data": [
                { "id": "claude-haiku-4-5", "max_input_tokens": 200000, "capabilities": {} }
            ],
            "has_more": false
        }"#;

        let response: ModelsResponse = serde_json::from_str(payload).expect("parses");
        let model = &response.data[0];

        assert_eq!(model.display_name, None);
        assert_eq!(model.max_tokens, None);
        assert!(!model.capabilities.image_input.supported);
        assert!(!model.capabilities.structured_outputs.supported);
        assert!(!model.capabilities.thinking.supported);
        assert_eq!(response.last_id, None);
    }
}

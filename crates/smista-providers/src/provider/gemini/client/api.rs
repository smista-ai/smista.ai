//! Gemini API types for the `v1beta/models` endpoint.

use serde::{Deserialize, Serialize};

/// One page of the `v1beta/models` listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsResponse {
    /// The models on this page.
    #[serde(default)]
    pub models: Vec<Model>,
    /// The cursor for the next page, present only while more pages follow.
    #[serde(default)]
    pub next_page_token: Option<String>,
}

/// A single model entry from the `v1beta/models` listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// The resource name, e.g. `models/gemini-2.5-flash`.
    pub name: String,
    /// A human-friendly name, when the API provides one.
    #[serde(default)]
    pub display_name: Option<String>,
    /// A free-form description of the model.
    #[serde(default)]
    pub description: Option<String>,
    /// Maximum number of input (context) tokens the model accepts.
    #[serde(default)]
    pub input_token_limit: u32,
    /// Maximum number of output tokens the model emits, when bounded.
    #[serde(default)]
    pub output_token_limit: Option<u32>,
    /// The generation methods the model supports, e.g. `generateContent`.
    ///
    /// This is how chat models are told apart from the embedding, image, audio
    /// and other specialised models the listing also returns.
    #[serde(default)]
    pub supported_generation_methods: Vec<String>,
    /// Whether the model performs explicit reasoning (thinking).
    #[serde(default)]
    pub thinking: bool,
}

impl Model {
    /// Returns the routable model id, stripping the `models/` resource prefix.
    ///
    /// The listing names a model as `models/<id>`, but smista.ai routes on the
    /// bare `<id>` (e.g. `gemini-2.5-flash`), so the prefix is removed here.
    pub fn id(&self) -> &str {
        self.name.strip_prefix("models/").unwrap_or(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_deserialize_a_model_entry() {
        // Trimmed from the issue's `v1beta/models` sample: the fields the adapter reads.
        let payload = r#"{
            "models": [
                {
                    "name": "models/gemini-2.5-flash",
                    "displayName": "Gemini 2.5 Flash",
                    "description": "A fast multimodal model.",
                    "inputTokenLimit": 1048576,
                    "outputTokenLimit": 65536,
                    "supportedGenerationMethods": ["generateContent", "countTokens"],
                    "thinking": true
                }
            ],
            "nextPageToken": "abc"
        }"#;

        let response: ModelsResponse = serde_json::from_str(payload).expect("parses");

        assert_eq!(response.models.len(), 1);
        let model = &response.models[0];
        assert_eq!(model.name, "models/gemini-2.5-flash");
        assert_eq!(model.id(), "gemini-2.5-flash");
        assert_eq!(model.display_name.as_deref(), Some("Gemini 2.5 Flash"));
        assert_eq!(model.input_token_limit, 1_048_576);
        assert_eq!(model.output_token_limit, Some(65_536));
        assert_eq!(
            model.supported_generation_methods,
            vec!["generateContent".to_string(), "countTokens".to_string()]
        );
        assert!(model.thinking);
        assert_eq!(response.next_page_token.as_deref(), Some("abc"));
    }

    #[test]
    fn should_default_absent_fields() {
        let payload = r#"{
            "models": [
                { "name": "models/text-embedding-004", "inputTokenLimit": 2048 }
            ]
        }"#;

        let response: ModelsResponse = serde_json::from_str(payload).expect("parses");
        let model = &response.models[0];

        assert_eq!(model.display_name, None);
        assert_eq!(model.description, None);
        assert_eq!(model.output_token_limit, None);
        assert!(model.supported_generation_methods.is_empty());
        assert!(!model.thinking);
        assert_eq!(response.next_page_token, None);
    }

    #[test]
    fn should_fall_back_to_the_full_name_when_the_prefix_is_absent() {
        let model = Model {
            name: "gemini-2.5-flash".to_string(),
            display_name: None,
            description: None,
            input_token_limit: 0,
            output_token_limit: None,
            supported_generation_methods: Vec::new(),
            thinking: false,
        };

        assert_eq!(model.id(), "gemini-2.5-flash");
    }
}

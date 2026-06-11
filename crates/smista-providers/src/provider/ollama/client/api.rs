//! Ollama API types.

use serde::{Deserialize, Serialize};

/// Response for the `/api/tags` endpoint.
///
/// This endpoint returns a list of available models and their parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TagsResponse {
    /// Available models
    pub models: Vec<Model>,
}

/// A model returned by the `/api/tags` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Model {
    /// Model display name
    pub name: String,
    /// Details
    pub details: Details,
    /// Model identifier
    pub model: String,
    /// Model capabilities, e.g. "chat", "embedding", "completion"
    pub capabilities: Vec<Capability>,
}

/// Model details
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Details {
    /// Context length in tokens
    pub context_length: u32,
}

/// Capability of a model, as returned by the `/api/tags` endpoint.
#[derive(Debug, Clone)]
pub enum Capability {
    Audio,
    Completion,
    Embedding,
    Image,
    Insert,
    Thinking,
    Tools,
    Vision,
    Other(String),
}

impl Serialize for Capability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            Self::Audio => "audio",
            Self::Completion => "completion",
            Self::Embedding => "embedding",
            Self::Image => "image",
            Self::Insert => "insert",
            Self::Thinking => "thinking",
            Self::Tools => "tools",
            Self::Vision => "vision",
            Self::Other(other) => other.as_str(),
        };
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "audio" => Ok(Self::Audio),
            "completion" => Ok(Self::Completion),
            "embedding" => Ok(Self::Embedding),
            "image" => Ok(Self::Image),
            "insert" => Ok(Self::Insert),
            "thinking" => Ok(Self::Thinking),
            "tools" => Ok(Self::Tools),
            "vision" => Ok(Self::Vision),
            other => Ok(Self::Other(other.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_every_known_capability_to_its_wire_string() {
        let cases = [
            (Capability::Audio, "\"audio\""),
            (Capability::Completion, "\"completion\""),
            (Capability::Embedding, "\"embedding\""),
            (Capability::Image, "\"image\""),
            (Capability::Insert, "\"insert\""),
            (Capability::Thinking, "\"thinking\""),
            (Capability::Tools, "\"tools\""),
            (Capability::Vision, "\"vision\""),
        ];

        for (capability, expected) in cases {
            let json = serde_json::to_string(&capability).expect("serializes");
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn should_serialize_unknown_capability_using_its_inner_string() {
        let json = serde_json::to_string(&Capability::Other("speculative".to_string()))
            .expect("serializes");
        assert_eq!(json, "\"speculative\"");
    }

    #[test]
    fn should_deserialize_known_capabilities() {
        let parsed: Capability = serde_json::from_str("\"tools\"").expect("parses");
        assert!(matches!(parsed, Capability::Tools));
    }

    #[test]
    fn should_deserialize_unknown_capability_into_other() {
        let parsed: Capability = serde_json::from_str("\"speculative\"").expect("parses");
        let Capability::Other(value) = parsed else {
            panic!("an unknown capability must land in `Other`");
        };
        assert_eq!(value, "speculative");
    }

    #[test]
    fn should_round_trip_known_and_unknown_capabilities() {
        for capability in [Capability::Vision, Capability::Other("audio2".to_string())] {
            let json = serde_json::to_string(&capability).expect("serializes");
            let back: Capability = serde_json::from_str(&json).expect("parses");
            assert_eq!(
                serde_json::to_string(&back).expect("re-serializes"),
                json,
                "round-trip changed the wire form"
            );
        }
    }

    #[test]
    fn should_deserialize_tags_response_from_the_documented_payload() {
        // Trimmed from the issue's `/api/tags` sample: the fields the adapter reads.
        let payload = r#"{
            "models": [
                {
                    "name": "qwen2.5-coder:14b",
                    "model": "qwen2.5-coder:14b",
                    "details": { "context_length": 32768 },
                    "capabilities": ["completion", "tools", "insert"]
                },
                {
                    "name": "devstral-small-2:latest",
                    "model": "devstral-small-2:latest",
                    "details": { "context_length": 393216 },
                    "capabilities": ["vision", "completion", "tools"]
                }
            ]
        }"#;

        let response: TagsResponse = serde_json::from_str(payload).expect("parses");

        assert_eq!(response.models.len(), 2);
        let first = &response.models[0];
        assert_eq!(first.model, "qwen2.5-coder:14b");
        assert_eq!(first.details.context_length, 32768);
        assert_eq!(first.capabilities.len(), 3);
        assert!(matches!(first.capabilities[1], Capability::Tools));
    }
}

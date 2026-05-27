//! Model capabilities and the requirements a task places on a model.
//!
//! [`ModelCapabilities`] describes what a model can do — whether it streams,
//! calls tools, accepts images, and so on. [`RoutingRequirements`] describes
//! what a task needs from a model. smista-router compares the two when
//! selecting a model, so that, for example, a task that calls tools is never
//! routed to a model that cannot.
//!
//! The capability flags are grouped under their own struct rather than being
//! flattened onto [`ModelDescriptor`](super::ModelDescriptor): capability
//! checks read more clearly against a single value, and the JSON API in #7 can
//! still expose a flat shape by serializing the nested object under a
//! `capabilities` key.
//!
//! # Examples
//!
//! ```
//! use smista_core::model::{Capability, ModelCapabilities};
//!
//! let caps = ModelCapabilities {
//!     streaming: true,
//!     tools: true,
//!     ..Default::default()
//! };
//! assert!(caps.supports(Capability::Tools));
//! assert!(!caps.supports(Capability::Images));
//! ```

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};

/// The capabilities a model exposes.
///
/// Every flag defaults to `false`, so a [`Default`] value advertises no
/// capabilities — a conservative starting point a descriptor builds upon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// The model can stream its response incrementally.
    #[serde(default)]
    pub streaming: bool,
    /// The model can call tools.
    #[serde(default)]
    pub tools: bool,
    /// The model can be constrained to emit JSON.
    #[serde(default)]
    pub json_output: bool,
    /// The model honors a separate system prompt.
    #[serde(default)]
    pub system_prompt: bool,
    /// The model accepts image inputs.
    #[serde(default)]
    pub images: bool,
    /// The model performs explicit reasoning.
    #[serde(default)]
    pub reasoning: bool,
}

impl ModelCapabilities {
    /// Returns whether the model supports `capability`.
    #[must_use]
    pub const fn supports(&self, capability: Capability) -> bool {
        match capability {
            Capability::Streaming => self.streaming,
            Capability::Tools => self.tools,
            Capability::JsonOutput => self.json_output,
            Capability::SystemPrompt => self.system_prompt,
            Capability::Images => self.images,
            Capability::Reasoning => self.reasoning,
        }
    }
}

/// A single model capability, used to report which one a task requires.
///
/// Each variant serializes to its lowercase snake_case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Incremental streaming of the response.
    Streaming,
    /// Tool calls.
    Tools,
    /// Constrained JSON output.
    JsonOutput,
    /// A separate system prompt.
    SystemPrompt,
    /// Image inputs.
    Images,
    /// Explicit reasoning.
    Reasoning,
}

impl Capability {
    /// Returns the snake_case string representation of the capability.
    ///
    /// This is the same form used for serialization and [`Display`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Tools => "tools",
            Self::JsonOutput => "json_output",
            Self::SystemPrompt => "system_prompt",
            Self::Images => "images",
            Self::Reasoning => "reasoning",
        }
    }
}

impl Display for Capability {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a task requires from the model it is routed to.
///
/// Each flag mirrors a field of [`ModelCapabilities`] and requests that
/// capability when set. `estimated_tokens`, when present, is checked against
/// the model's context window. A [`Default`] value requires nothing, so an
/// unconstrained task can be handled by any model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RoutingRequirements {
    /// The task needs the response streamed.
    pub streaming: bool,
    /// The task calls tools.
    pub tools: bool,
    /// The task needs JSON output.
    pub json_output: bool,
    /// The task uses a system prompt.
    pub system_prompt: bool,
    /// The task supplies image inputs.
    pub images: bool,
    /// The task needs explicit reasoning.
    pub reasoning: bool,
    /// Estimated number of input tokens, checked against the context window.
    pub estimated_tokens: Option<u64>,
}

impl RoutingRequirements {
    /// Returns the capabilities the task requires, in declaration order.
    pub(crate) fn required(&self) -> impl Iterator<Item = Capability> {
        [
            (self.streaming, Capability::Streaming),
            (self.tools, Capability::Tools),
            (self.json_output, Capability::JsonOutput),
            (self.system_prompt, Capability::SystemPrompt),
            (self.images, Capability::Images),
            (self.reasoning, Capability::Reasoning),
        ]
        .into_iter()
        .filter_map(|(needed, capability)| needed.then_some(capability))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Capability; 6] = [
        Capability::Streaming,
        Capability::Tools,
        Capability::JsonOutput,
        Capability::SystemPrompt,
        Capability::Images,
        Capability::Reasoning,
    ];

    #[test]
    fn should_default_to_no_capabilities() {
        let caps = ModelCapabilities::default();
        assert!(ALL.into_iter().all(|cap| !caps.supports(cap)));
    }

    #[test]
    fn should_report_supported_capability() {
        let caps = ModelCapabilities {
            tools: true,
            ..Default::default()
        };
        assert!(caps.supports(Capability::Tools));
        assert!(!caps.supports(Capability::Streaming));
    }

    #[test]
    fn should_serialize_capability_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&Capability::JsonOutput).unwrap(),
            "\"json_output\""
        );
    }

    #[test]
    fn should_match_display_with_serde_representation() {
        for capability in ALL {
            let json = serde_json::to_string(&capability).unwrap();
            assert_eq!(json, format!("\"{capability}\""));
        }
    }

    #[test]
    fn should_default_to_no_requirements() {
        assert_eq!(RoutingRequirements::default().required().count(), 0);
    }

    #[test]
    fn should_list_required_capabilities_in_declaration_order() {
        let requirements = RoutingRequirements {
            tools: true,
            reasoning: true,
            ..Default::default()
        };
        let required: Vec<_> = requirements.required().collect();
        assert_eq!(required, vec![Capability::Tools, Capability::Reasoning]);
    }
}

//! Error types for the Smista core library.

use crate::model::Capability;

/// Error types for the Smista core library.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum SmistaError {
    /// A model could not satisfy a task's routing requirements.
    #[error(transparent)]
    Capability(#[from] CapabilityError),
    /// A textual value could not be parsed into a domain type.
    #[error(transparent)]
    Parse(#[from] ParseError),
}

/// Errors produced when parsing a domain type from its textual form.
///
/// These are returned by the [`FromStr`](std::str::FromStr) and `serde`
/// implementations of the core domain types when given an unrecognized value.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum ParseError {
    /// A model reference was not in the expected `provider/model` form.
    #[error("invalid model reference: {0}")]
    InvalidModelReference(String),
    /// A reasoning effort name was not recognized.
    #[error("unknown effort: {0}")]
    UnknownEffort(String),
    /// A task intent name was not recognized.
    #[error("unknown task intent: {0}")]
    UnknownIntent(String),
    /// A provider identifier was not recognized.
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
}

/// Errors produced when a model cannot satisfy a task's routing requirements.
///
/// Returned by
/// [`ModelDescriptor::can_handle`](crate::model::ModelDescriptor::can_handle)
/// when a task needs a capability the model lacks, or when the estimated input
/// exceeds the model's context window.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CapabilityError {
    /// The estimated input exceeds the model's context window.
    #[error(
        "estimated {estimated_tokens} tokens exceed the model context window of {max_context_tokens}"
    )]
    ContextWindowExceeded {
        /// Estimated number of input tokens for the task.
        estimated_tokens: u64,
        /// Maximum number of context tokens the model accepts.
        max_context_tokens: u32,
    },
    /// The model does not support a capability the task requires.
    #[error("model does not support required capability: {0}")]
    MissingCapability(Capability),
}

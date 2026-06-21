//! Error types for the Smista core library.
//!
//! The root [`CoreError`] enum is the single error type produced by the
//! library and surfaced through the router HTTP API. It groups domain
//! sub-errors by area (authentication, parsing, policy, routing, provider
//! interaction) and offers a generic [`CoreError::Internal`] variant for
//! opaque failures that must not leak details to API clients.
//!
//! Sub-error types are independent and can be used directly within the crate.
//! Conversion into the API wire form is provided by the
//! `From<CoreError> for ApiErrorResponse` implementation alongside
//! [`ApiError`](crate::api::ApiError).

use serde::{Deserialize, Serialize};

use crate::model::{Capability, Provider};
use crate::policy::PermissionMode;

/// Root error type produced by `smista-core`.
///
/// Every fallible public operation in the crate returns this type, either
/// directly or via a sub-error that converts into it through `#[from]`.
///
/// The [`Internal`](CoreError::Internal) variant carries a free-form message
/// describing an unexpected condition. Its content must never include
/// secrets, since the message can appear in logs.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CoreError {
    /// Authentication or authorization failed.
    #[error(transparent)]
    Auth(#[from] AuthError),
    /// A model could not satisfy a task's routing requirements.
    #[error(transparent)]
    Capability(#[from] CapabilityError),
    /// An unexpected internal failure occurred; details are opaque to API
    /// clients and meant for logs only.
    #[error("internal error: {0}")]
    Internal(String),
    /// A textual value could not be parsed into a domain type.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// A policy was internally inconsistent.
    #[error(transparent)]
    Policy(#[from] PolicyError),
    /// A model provider reported an error.
    #[error(transparent)]
    Provider(#[from] ProviderError),
    /// The router could not select a model for a task.
    #[error(transparent)]
    Routing(#[from] RoutingError),
}

/// Errors raised by the authentication and authorization layer.
///
/// These are produced when the router validates an API key or session token
/// from an HTTP request. Each variant maps to a stable HTTP status code by
/// the API mapping alongside [`ApiError`](crate::api::ApiError).
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum AuthError {
    /// The presented token is past its expiry timestamp.
    #[error("token expired")]
    Expired,
    /// The caller is authenticated but not allowed to access the resource
    /// (for example, it is not the resource owner).
    #[error("forbidden: not the resource owner")]
    Forbidden,
    /// The presented token is malformed or does not match any known credential.
    #[error("invalid token")]
    InvalidToken,
    /// No credentials were presented when the endpoint requires them.
    #[error("missing credentials")]
    MissingCredentials,
    /// The presented token was previously valid but has been revoked.
    #[error("token revoked")]
    Revoked,
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
    /// An OpenAI-compatible instance name contained unsupported characters.
    #[error("invalid provider instance name: {0}")]
    InvalidProviderName(String),
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

/// Errors produced when evaluating or merging policy.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum PolicyError {
    /// An override tried to loosen a tool permission it may only tighten.
    #[error("tool '{tool}' permission '{requested}' would loosen the stricter '{base}'")]
    PermissionExpansion {
        /// The stricter base mode that must not be weakened.
        base: PermissionMode,
        /// The looser mode the override requested.
        requested: PermissionMode,
        /// The tool whose permission was loosened.
        tool: String,
    },
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

/// Errors raised by the deterministic router when selecting a model for a
/// task.
///
/// These describe routing-time decisions, distinct from
/// [`CapabilityError`] (which is raised when checking whether a known model
/// can handle a task) and [`ProviderError`] (which describes failures coming
/// back from a provider call).
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum RoutingError {
    /// A primary route failed and every configured fallback was also
    /// exhausted without success.
    #[error("fallback chain exhausted")]
    FallbackExhausted,
    /// No routing rule matched and no default route is configured.
    #[error("no rule matched and no default route")]
    NoRoute,
    /// The caller requested an explicit model override that policy forbids.
    #[error("explicit model override `{0}` not allowed by policy")]
    OverrideNotAllowed(String),
    /// The selected model is unknown to the router (not configured).
    #[error("model `{0}` not configured")]
    UnknownModel(String),
    /// The selected model lacks a capability the task requires.
    #[error("selected model lacks required capability: {0}")]
    UnsupportedCapability(Capability),
}

/// A normalized error returned by a model provider.
///
/// Provider adapters in `smista-providers` translate provider-specific error
/// shapes (HTTP statuses, SDK error variants) into this common form. The
/// router uses the [`ProviderErrorCategory`] to decide whether to attempt a
/// fallback model, and the API layer surfaces the category as a stable
/// machine-readable code.
///
/// The `message` field is provider-agnostic and must never embed credentials
/// or other secret values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("provider `{provider}` reported {category}: {message}")]
pub struct ProviderError {
    /// The classification of the failure, used for routing and API mapping.
    pub category: ProviderErrorCategory,
    /// Human-readable, redacted explanation of what the provider reported.
    pub message: String,
    /// The provider that produced the error.
    pub provider: Provider,
    /// The model that was being called, when known.
    pub model: Option<String>,
}

/// Classification of a normalized [`ProviderError`].
///
/// Each variant captures a category of failure that the router and API layer
/// can reason about independently of the provider-specific HTTP status or
/// SDK error code that produced it. The categories follow the smista.ai
/// specification (Model interfaces → Error handling).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error, ts_rs::TS,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[ts(export)]
#[ts(rename_all = "snake_case")]
pub enum ProviderErrorCategory {
    /// The request was authenticated but rejected (for example, a quota
    /// rule, plan limit, or organization policy denied it).
    #[error("authenticated request rejected by provider policy")]
    Authentication,
    /// The request exceeded the model's context window.
    #[error("request exceeded the model context window")]
    ContextLength,
    /// The provider was configured with contradictory or invalid settings
    /// (for example, an OpenAI-compatible instance whose declared locality
    /// disagrees with one of its models). Detected when the provider is built,
    /// not while serving a request.
    #[error("provider configuration is invalid")]
    InvalidConfiguration,
    /// Credentials were provided but rejected by the provider.
    #[error("provider rejected the configured credentials")]
    InvalidCredentials,
    /// The request body was malformed or referenced an unknown parameter.
    #[error("provider rejected the request as malformed")]
    InvalidRequest,
    /// No credentials were configured for the provider when the call was made.
    #[error("no credentials configured for the provider")]
    MissingCredentials,
    /// The referenced model is not offered by the provider that was asked to
    /// resolve it.
    #[error("model not offered by the provider")]
    ModelNotFound,
    /// The provider returned a service-level error and may recover.
    #[error("provider is unavailable")]
    ProviderUnavailable,
    /// The provider rate-limited the request.
    #[error("provider rate-limited the request")]
    RateLimit,
    /// Storage error occurred while attempting to save or retrieve data related to the provider call.
    #[error("storage error")]
    Storage,
    /// The call timed out before the provider returned a response.
    #[error("call to the provider timed out")]
    Timeout,
    /// The error did not match any known category.
    #[error("unknown provider error")]
    Unknown,
    /// The model does not support a capability the request required.
    #[error("model does not support a required capability")]
    UnsupportedCapability,
}

impl ProviderErrorCategory {
    /// Returns `true` when an error of this category may be retried against
    /// a different model via the router's fallback chain.
    ///
    /// Fallback is allowed for transient or capacity-related failures
    /// ([`RateLimit`](Self::RateLimit),
    /// [`ProviderUnavailable`](Self::ProviderUnavailable),
    /// [`Timeout`](Self::Timeout),
    /// [`Unknown`](Self::Unknown)), and forbidden for failures caused by the
    /// request or credentials being wrong (which would just fail again).
    #[must_use]
    pub const fn is_fallback_eligible(self) -> bool {
        matches!(
            self,
            Self::ProviderUnavailable | Self::RateLimit | Self::Timeout | Self::Unknown,
        )
    }
}

impl From<http::StatusCode> for ProviderErrorCategory {
    fn from(status: http::StatusCode) -> Self {
        match status {
            http::StatusCode::BAD_REQUEST => Self::InvalidRequest,
            http::StatusCode::UNAUTHORIZED => Self::InvalidCredentials,
            http::StatusCode::FORBIDDEN => Self::Authentication,
            http::StatusCode::TOO_MANY_REQUESTS => Self::RateLimit,
            status if status.is_server_error() => Self::ProviderUnavailable,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_make_transient_categories_fallback_eligible() {
        assert!(ProviderErrorCategory::RateLimit.is_fallback_eligible());
        assert!(ProviderErrorCategory::ProviderUnavailable.is_fallback_eligible());
        assert!(ProviderErrorCategory::Timeout.is_fallback_eligible());
        assert!(ProviderErrorCategory::Unknown.is_fallback_eligible());
    }

    #[test]
    fn should_make_deterministic_categories_not_fallback_eligible() {
        assert!(!ProviderErrorCategory::Authentication.is_fallback_eligible());
        assert!(!ProviderErrorCategory::InvalidCredentials.is_fallback_eligible());
        assert!(!ProviderErrorCategory::MissingCredentials.is_fallback_eligible());
        assert!(!ProviderErrorCategory::ModelNotFound.is_fallback_eligible());
        assert!(!ProviderErrorCategory::InvalidRequest.is_fallback_eligible());
        assert!(!ProviderErrorCategory::ContextLength.is_fallback_eligible());
        assert!(!ProviderErrorCategory::UnsupportedCapability.is_fallback_eligible());
    }

    #[test]
    fn should_serialize_category_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&ProviderErrorCategory::ProviderUnavailable).unwrap(),
            "\"provider_unavailable\"",
        );
        assert_eq!(
            serde_json::to_string(&ProviderErrorCategory::ContextLength).unwrap(),
            "\"context_length\"",
        );
    }

    #[test]
    fn should_convert_sub_errors_into_core_error() {
        let auth: CoreError = AuthError::Expired.into();
        assert!(matches!(auth, CoreError::Auth(AuthError::Expired)));

        let routing: CoreError = RoutingError::NoRoute.into();
        assert!(matches!(routing, CoreError::Routing(RoutingError::NoRoute)));

        let provider: CoreError = ProviderError {
            category: ProviderErrorCategory::RateLimit,
            message: "rate limited".to_string(),
            provider: Provider::Anthropic,
            model: Some("claude-sonnet".to_string()),
        }
        .into();
        assert!(matches!(provider, CoreError::Provider(_)));
    }

    #[test]
    fn should_map_client_status_codes_to_categories() {
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::BAD_REQUEST),
            ProviderErrorCategory::InvalidRequest,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::UNAUTHORIZED),
            ProviderErrorCategory::InvalidCredentials,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::FORBIDDEN),
            ProviderErrorCategory::Authentication,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::TOO_MANY_REQUESTS),
            ProviderErrorCategory::RateLimit,
        );
    }

    #[test]
    fn should_map_server_status_codes_to_provider_unavailable() {
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::INTERNAL_SERVER_ERROR),
            ProviderErrorCategory::ProviderUnavailable,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::BAD_GATEWAY),
            ProviderErrorCategory::ProviderUnavailable,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::SERVICE_UNAVAILABLE),
            ProviderErrorCategory::ProviderUnavailable,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::GATEWAY_TIMEOUT),
            ProviderErrorCategory::ProviderUnavailable,
        );
    }

    #[test]
    fn should_map_unhandled_status_codes_to_unknown() {
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::OK),
            ProviderErrorCategory::Unknown,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::NOT_FOUND),
            ProviderErrorCategory::Unknown,
        );
        assert_eq!(
            ProviderErrorCategory::from(http::StatusCode::CONFLICT),
            ProviderErrorCategory::Unknown,
        );
    }

    #[test]
    fn should_render_provider_error_display_without_internal_fields() {
        let error = ProviderError {
            category: ProviderErrorCategory::RateLimit,
            message: "rate limited".to_string(),
            provider: Provider::Anthropic,
            model: None,
        };
        let rendered = error.to_string();
        assert!(rendered.contains("anthropic"));
        assert!(rendered.contains("rate limited"));
    }
}

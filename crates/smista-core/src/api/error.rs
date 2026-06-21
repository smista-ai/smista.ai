//! Structured error body returned by failing API requests.
//!
//! Every non-success response carries an [`ApiError`] wrapping an
//! [`ApiErrorBody`] with a stable machine-readable `code`, a human-readable
//! `message`, and optional structured `details`. The `code` (for example
//! `missing_provider_credentials`) is the part clients match on; the HTTP
//! status conveys the broad outcome.
//!
//! Error bodies must never carry secrets: provider credentials, session tokens
//! and API keys are redacted before an error is constructed.
//!
//! [`ApiErrorResponse`] pairs an [`ApiError`] with the HTTP
//! [`StatusCode`](http::StatusCode) the router should return. It is produced
//! from a [`CoreError`] via the `From` implementation in this module, which
//! is the single source of truth for mapping the crate's domain errors to
//! the wire format.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::{ApiError, ApiErrorBody};
//!
//! let error = ApiError {
//!     error: ApiErrorBody {
//!         code: "missing_provider_credentials".to_string(),
//!         message: "The selected model requires provider credentials.".to_string(),
//!         details: Some(serde_json::json!({ "provider": "anthropic" })),
//!     },
//! };
//! let json = serde_json::to_string(&error).unwrap();
//! assert!(json.contains("\"code\":\"missing_provider_credentials\""));
//! ```

use std::fmt;

use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::error::{
    AuthError, CapabilityError, CoreError, ParseError, PolicyError, ProviderError,
    ProviderErrorCategory, RoutingError,
};

/// The stable, machine-readable error codes carried in [`ApiErrorBody::code`].
///
/// The code is the part clients match on; it never changes for a given failure
/// mode. Each code maps to exactly one HTTP status, returned by
/// [`ApiErrorCode::status`], and to one stable wire string, returned by
/// [`ApiErrorCode::as_str`]. This enum is the authoritative list of codes the
/// router can emit and mirrors the table in `docs/api/http-api.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiErrorCode {
    /// Request exceeds the provider model's context window.
    ContextLengthExceeded,
    /// Routing rejected a model whose context window cannot fit the input.
    ContextWindowExceeded,
    /// A credential was smuggled through a query parameter instead of a header.
    CredentialsInQuery,
    /// Primary route failed and every configured fallback also failed.
    FallbackExhausted,
    /// Caller is authenticated but not the resource owner.
    Forbidden,
    /// Unexpected server-side failure. Details are intentionally omitted.
    InternalError,
    /// The API key presented at sign-in is malformed, unknown or does not match.
    InvalidApiKey,
    /// A model reference was not in the expected `provider/model` form.
    InvalidModelReference,
    /// A provider was configured with contradictory settings.
    InvalidProviderConfiguration,
    /// The provider rejected the configured credentials.
    InvalidProviderCredentials,
    /// A provider identifier was not in the expected form.
    InvalidProviderName,
    /// The provider rejected the request body as malformed.
    InvalidRequest,
    /// The session token is malformed or unknown.
    InvalidToken,
    /// The selected model lacks a capability the task requires.
    MissingCapability,
    /// No session token was presented to a protected endpoint.
    MissingCredentials,
    /// The selected model requires provider credentials none were configured.
    MissingProviderCredentials,
    /// The referenced model is not offered by the provider asked to resolve it.
    ModelNotFound,
    /// No routing rule matched and no default route is configured.
    NoRoute,
    /// The endpoint is recognized but not implemented yet.
    NotImplemented,
    /// The caller asked for a model override that policy forbids.
    OverrideNotAllowed,
    /// An override tried to loosen a tool permission that may only be tightened.
    PermissionExpansion,
    /// The provider rejected the request at the authentication layer.
    ProviderAuthentication,
    /// The provider returned an error that did not match any known category.
    ProviderError,
    /// The provider returned a service-level error and may recover later.
    ProviderUnavailable,
    /// The provider reported it does not support a needed capability.
    ProviderUnsupportedCapability,
    /// The provider rate-limited the request.
    RateLimited,
    /// The call to the provider timed out before a response was returned.
    RequestTimeout,
    /// Routing rejected the selected model because it lacks a capability.
    RoutingUnsupportedCapability,
    /// An error occurred while reading or writing from memory storage.
    StorageError,
    /// The session token is past its expiry timestamp.
    TokenExpired,
    /// The session token was previously valid but has been revoked.
    TokenRevoked,
    /// A reasoning effort name in the request was not recognized.
    UnknownEffort,
    /// A task intent name in the request was not recognized.
    UnknownIntent,
    /// A referenced model is not configured on the router.
    UnknownModel,
    /// A provider identifier in the request was not recognized.
    UnknownProvider,
}

impl ApiErrorCode {
    /// Returns the stable wire string clients match on, such as `invalid_token`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContextLengthExceeded => "context_length_exceeded",
            Self::ContextWindowExceeded => "context_window_exceeded",
            Self::CredentialsInQuery => "credentials_in_query",
            Self::FallbackExhausted => "fallback_exhausted",
            Self::Forbidden => "forbidden",
            Self::InternalError => "internal_error",
            Self::InvalidApiKey => "invalid_api_key",
            Self::InvalidModelReference => "invalid_model_reference",
            Self::InvalidProviderConfiguration => "invalid_provider_configuration",
            Self::InvalidProviderCredentials => "invalid_provider_credentials",
            Self::InvalidProviderName => "invalid_provider_name",
            Self::InvalidRequest => "invalid_request",
            Self::InvalidToken => "invalid_token",
            Self::MissingCapability => "missing_capability",
            Self::MissingCredentials => "missing_credentials",
            Self::MissingProviderCredentials => "missing_provider_credentials",
            Self::ModelNotFound => "model_not_found",
            Self::NoRoute => "no_route",
            Self::NotImplemented => "not_implemented",
            Self::OverrideNotAllowed => "override_not_allowed",
            Self::PermissionExpansion => "permission_expansion",
            Self::ProviderAuthentication => "provider_authentication",
            Self::ProviderError => "provider_error",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ProviderUnsupportedCapability => "provider_unsupported_capability",
            Self::RateLimited => "rate_limited",
            Self::RequestTimeout => "request_timeout",
            Self::RoutingUnsupportedCapability => "routing_unsupported_capability",
            Self::StorageError => "storage_error",
            Self::TokenExpired => "token_expired",
            Self::TokenRevoked => "token_revoked",
            Self::UnknownEffort => "unknown_effort",
            Self::UnknownIntent => "unknown_intent",
            Self::UnknownModel => "unknown_model",
            Self::UnknownProvider => "unknown_provider",
        }
    }

    /// Returns the HTTP status that always accompanies this code.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::ContextLengthExceeded
            | Self::ContextWindowExceeded
            | Self::InvalidModelReference
            | Self::InvalidProviderName
            | Self::InvalidRequest
            | Self::MissingCapability
            | Self::NoRoute
            | Self::PermissionExpansion
            | Self::ProviderUnsupportedCapability
            | Self::RoutingUnsupportedCapability
            | Self::UnknownEffort
            | Self::UnknownIntent
            | Self::UnknownModel
            | Self::UnknownProvider => StatusCode::UNPROCESSABLE_ENTITY,
            Self::CredentialsInQuery => StatusCode::BAD_REQUEST,
            Self::NotImplemented => StatusCode::NOT_IMPLEMENTED,
            Self::FallbackExhausted
            | Self::InvalidProviderCredentials
            | Self::MissingProviderCredentials
            | Self::ProviderAuthentication
            | Self::ProviderUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Forbidden | Self::OverrideNotAllowed => StatusCode::FORBIDDEN,
            Self::InternalError | Self::InvalidProviderConfiguration => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Self::InvalidApiKey
            | Self::InvalidToken
            | Self::MissingCredentials
            | Self::TokenExpired
            | Self::TokenRevoked => StatusCode::UNAUTHORIZED,
            Self::ModelNotFound => StatusCode::NOT_FOUND,
            Self::ProviderError | Self::StorageError => StatusCode::BAD_GATEWAY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::RequestTimeout => StatusCode::GATEWAY_TIMEOUT,
        }
    }
}

impl fmt::Display for ApiErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Envelope wrapping the error body under an `error` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct ApiError {
    /// The structured error payload.
    pub error: ApiErrorBody,
}

/// The structured payload of an [`ApiError`].
///
/// `details` carries machine-readable context (such as the offending provider
/// and model) and is omitted when there is none. It must never contain secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct ApiErrorBody {
    /// Stable machine-readable error code, for example `missing_provider_credentials`.
    pub code: String,
    /// Human-readable, actionable description of what went wrong.
    pub message: String,
    /// Optional structured context; never carries secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[cfg_attr(feature = "openapi", schema(value_type = Object, nullable = true))]
    pub details: Option<serde_json::Value>,
}

/// An [`ApiError`] paired with the HTTP status code the router should return.
///
/// The web layer turns this into an `axum` response; the
/// [`From<CoreError>`](#impl-From<CoreError>-for-ApiErrorResponse)
/// implementation is the single source of truth for the mapping from
/// domain errors to the wire shape and status code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiErrorResponse {
    /// The HTTP status code that should accompany the response.
    pub status: StatusCode,
    /// The structured error body to serialize as JSON.
    pub body: ApiError,
}

impl ApiErrorResponse {
    /// Creates a new [`ApiErrorResponse`] with the given status, code and message, and no details.
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiError {
                error: ApiErrorBody {
                    code: code.to_string(),
                    message: message.into(),
                    details: None,
                },
            },
        }
    }

    /// Creates an [`ApiErrorResponse`] from a typed [`ApiErrorCode`], using the
    /// code's canonical HTTP status and the given message, and no details.
    pub fn from_code(code: ApiErrorCode, message: impl Into<String>) -> Self {
        Self::new(code.status(), code.as_str(), message)
    }

    /// Adds details to the error response, returning a new instance.
    /// Details should be machine-readable context about the error, and must never contain secrets.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.body.error.details = Some(details);
        self
    }
}

impl From<CoreError> for ApiErrorResponse {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Auth(error) => map_auth(&error),
            CoreError::Capability(error) => map_capability(&error),
            CoreError::Internal(_) => {
                Self::from_code(ApiErrorCode::InternalError, "An internal error occurred.")
            }
            CoreError::Parse(error) => map_parse(&error),
            CoreError::Policy(error) => map_policy(&error),
            CoreError::Provider(error) => map_provider(error),
            CoreError::Routing(error) => map_routing(error),
        }
    }
}

fn map_auth(error: &AuthError) -> ApiErrorResponse {
    let message = error.to_string();
    let code = match error {
        AuthError::Expired => ApiErrorCode::TokenExpired,
        AuthError::Forbidden => ApiErrorCode::Forbidden,
        AuthError::InvalidToken => ApiErrorCode::InvalidToken,
        AuthError::MissingCredentials => ApiErrorCode::MissingCredentials,
        AuthError::Revoked => ApiErrorCode::TokenRevoked,
    };
    ApiErrorResponse::from_code(code, message)
}

fn map_capability(error: &CapabilityError) -> ApiErrorResponse {
    let message = error.to_string();
    match error {
        CapabilityError::ContextWindowExceeded {
            estimated_tokens,
            max_context_tokens,
        } => ApiErrorResponse::from_code(ApiErrorCode::ContextWindowExceeded, message)
            .with_details(serde_json::json!({
                "estimated_tokens": estimated_tokens,
                "max_context_tokens": max_context_tokens,
            })),
        CapabilityError::MissingCapability(capability) => {
            ApiErrorResponse::from_code(ApiErrorCode::MissingCapability, message)
                .with_details(serde_json::json!({ "capability": capability }))
        }
    }
}

fn map_parse(error: &ParseError) -> ApiErrorResponse {
    let message = error.to_string();
    let code = match error {
        ParseError::InvalidModelReference(_) => ApiErrorCode::InvalidModelReference,
        ParseError::InvalidProviderName(_) => ApiErrorCode::InvalidProviderName,
        ParseError::UnknownEffort(_) => ApiErrorCode::UnknownEffort,
        ParseError::UnknownIntent(_) => ApiErrorCode::UnknownIntent,
        ParseError::UnknownProvider(_) => ApiErrorCode::UnknownProvider,
    };
    ApiErrorResponse::from_code(code, message)
}

fn map_policy(error: &PolicyError) -> ApiErrorResponse {
    let message = error.to_string();
    match error {
        PolicyError::PermissionExpansion {
            base,
            requested,
            tool,
        } => ApiErrorResponse::from_code(ApiErrorCode::PermissionExpansion, message).with_details(
            serde_json::json!({
                "tool": tool,
                "base": base,
                "requested": requested,
            }),
        ),
    }
}

fn map_routing(error: RoutingError) -> ApiErrorResponse {
    let message = error.to_string();
    match error {
        RoutingError::FallbackExhausted => {
            ApiErrorResponse::from_code(ApiErrorCode::FallbackExhausted, message)
        }
        RoutingError::NoRoute => ApiErrorResponse::from_code(ApiErrorCode::NoRoute, message),
        RoutingError::OverrideNotAllowed(model) => {
            ApiErrorResponse::from_code(ApiErrorCode::OverrideNotAllowed, message)
                .with_details(serde_json::json!({ "model": model }))
        }
        RoutingError::UnknownModel(model) => {
            ApiErrorResponse::from_code(ApiErrorCode::UnknownModel, message)
                .with_details(serde_json::json!({ "model": model }))
        }
        RoutingError::UnsupportedCapability(capability) => {
            ApiErrorResponse::from_code(ApiErrorCode::RoutingUnsupportedCapability, message)
                .with_details(serde_json::json!({ "capability": capability }))
        }
    }
}

fn map_provider(error: ProviderError) -> ApiErrorResponse {
    let message = error.to_string();
    let code = match error.category {
        ProviderErrorCategory::Authentication => ApiErrorCode::ProviderAuthentication,
        ProviderErrorCategory::ContextLength => ApiErrorCode::ContextLengthExceeded,
        ProviderErrorCategory::InvalidConfiguration => ApiErrorCode::InvalidProviderConfiguration,
        ProviderErrorCategory::InvalidCredentials => ApiErrorCode::InvalidProviderCredentials,
        ProviderErrorCategory::InvalidRequest => ApiErrorCode::InvalidRequest,
        ProviderErrorCategory::MissingCredentials => ApiErrorCode::MissingProviderCredentials,
        ProviderErrorCategory::ModelNotFound => ApiErrorCode::ModelNotFound,
        ProviderErrorCategory::ProviderUnavailable => ApiErrorCode::ProviderUnavailable,
        ProviderErrorCategory::RateLimit => ApiErrorCode::RateLimited,
        ProviderErrorCategory::Storage => ApiErrorCode::StorageError,
        ProviderErrorCategory::Timeout => ApiErrorCode::RequestTimeout,
        ProviderErrorCategory::Unknown => ApiErrorCode::ProviderError,
        ProviderErrorCategory::UnsupportedCapability => ApiErrorCode::ProviderUnsupportedCapability,
    };
    let mut details = serde_json::json!({ "provider": error.provider });
    if let Some(model) = error.model {
        details["model"] = serde_json::Value::String(model);
    }
    ApiErrorResponse::from_code(code, message).with_details(details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Capability, Provider};
    use crate::policy::PermissionMode;

    #[test]
    fn should_build_response_from_typed_code() {
        let response = ApiErrorResponse::from_code(ApiErrorCode::InvalidApiKey, "nope");
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
        assert_eq!(response.body.error.code, "invalid_api_key");
        assert_eq!(response.body.error.message, "nope");
        assert!(response.body.error.details.is_none());
    }

    #[test]
    fn should_render_code_as_its_wire_string() {
        assert_eq!(ApiErrorCode::InternalError.as_str(), "internal_error");
        assert_eq!(ApiErrorCode::InternalError.to_string(), "internal_error");
        assert_eq!(
            ApiErrorCode::ProviderUnsupportedCapability.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn should_construct_response_with_code_and_message() {
        let response = ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "The input was invalid.",
        );
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert_eq!(response.body.error.code, "invalid_input");
        assert_eq!(response.body.error.message, "The input was invalid.");
        assert!(response.body.error.details.is_none());
    }

    #[test]
    fn should_add_details_to_response() {
        let response = ApiErrorResponse::new(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "The input was invalid.",
        )
        .with_details(serde_json::json!({ "field": "name" }));
        assert_eq!(response.status, StatusCode::BAD_REQUEST);
        assert_eq!(response.body.error.code, "invalid_input");
        assert_eq!(response.body.error.message, "The input was invalid.");
        assert_eq!(
            response.body.error.details.as_ref().unwrap(),
            &serde_json::json!({ "field": "name" })
        );
    }

    #[test]
    fn should_serialize_to_spec_shape() {
        let error = ApiError {
            error: ApiErrorBody {
                code: "missing_provider_credentials".to_string(),
                message:
                    "The selected model requires provider credentials, but none were provided."
                        .to_string(),
                details: Some(serde_json::json!({
                    "provider": "anthropic",
                    "model": "claude-sonnet",
                })),
            },
        };
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({
                "error": {
                    "code": "missing_provider_credentials",
                    "message": "The selected model requires provider credentials, but none were provided.",
                    "details": { "provider": "anthropic", "model": "claude-sonnet" },
                }
            })
        );
    }

    #[test]
    fn should_omit_absent_details() {
        let error = ApiError {
            error: ApiErrorBody {
                code: "not_found".to_string(),
                message: "Session not found.".to_string(),
                details: None,
            },
        };
        let value = serde_json::to_value(&error).unwrap();
        assert!(value["error"].get("details").is_none());
    }

    #[test]
    fn should_roundtrip_serde() {
        let error = ApiError {
            error: ApiErrorBody {
                code: "rate_limited".to_string(),
                message: "Too many requests.".to_string(),
                details: None,
            },
        };
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(serde_json::from_str::<ApiError>(&json).unwrap(), error);
    }

    #[test]
    fn should_map_auth_errors_to_401_and_403() {
        let cases = [
            (
                AuthError::MissingCredentials,
                "missing_credentials",
                StatusCode::UNAUTHORIZED,
            ),
            (
                AuthError::InvalidToken,
                "invalid_token",
                StatusCode::UNAUTHORIZED,
            ),
            (
                AuthError::Expired,
                "token_expired",
                StatusCode::UNAUTHORIZED,
            ),
            (
                AuthError::Revoked,
                "token_revoked",
                StatusCode::UNAUTHORIZED,
            ),
            (AuthError::Forbidden, "forbidden", StatusCode::FORBIDDEN),
        ];
        for (error, code, status) in cases {
            let response = ApiErrorResponse::from(CoreError::from(error));
            assert_eq!(response.status, status);
            assert_eq!(response.body.error.code, code);
        }
    }

    #[test]
    fn should_map_parse_errors_to_422() {
        let response = ApiErrorResponse::from(CoreError::from(ParseError::UnknownIntent(
            "draft".to_string(),
        )));
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.body.error.code, "unknown_intent");
    }

    #[test]
    fn should_map_policy_errors_to_422_with_details() {
        let response = ApiErrorResponse::from(CoreError::from(PolicyError::PermissionExpansion {
            base: PermissionMode::Ask,
            requested: PermissionMode::Allow,
            tool: "fs.read".to_string(),
        }));
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.body.error.code, "permission_expansion");
        let details = response.body.error.details.as_ref().unwrap();
        assert_eq!(details["tool"], "fs.read");
    }

    #[test]
    fn should_map_capability_context_window_to_422_with_details() {
        let response =
            ApiErrorResponse::from(CoreError::from(CapabilityError::ContextWindowExceeded {
                estimated_tokens: 200_000,
                max_context_tokens: 128_000,
            }));
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.body.error.code, "context_window_exceeded");
        let details = response.body.error.details.as_ref().unwrap();
        assert_eq!(details["estimated_tokens"], 200_000);
        assert_eq!(details["max_context_tokens"], 128_000);
    }

    #[test]
    fn should_map_routing_errors_to_expected_status_codes() {
        let no_route = ApiErrorResponse::from(CoreError::from(RoutingError::NoRoute));
        assert_eq!(no_route.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(no_route.body.error.code, "no_route");

        let unknown =
            ApiErrorResponse::from(CoreError::from(RoutingError::UnknownModel("x".to_string())));
        assert_eq!(unknown.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(unknown.body.error.code, "unknown_model");
        assert_eq!(unknown.body.error.details.as_ref().unwrap()["model"], "x");

        let override_not_allowed = ApiErrorResponse::from(CoreError::from(
            RoutingError::OverrideNotAllowed("anthropic/claude".to_string()),
        ));
        assert_eq!(override_not_allowed.status, StatusCode::FORBIDDEN);
        assert_eq!(override_not_allowed.body.error.code, "override_not_allowed");

        let exhausted = ApiErrorResponse::from(CoreError::from(RoutingError::FallbackExhausted));
        assert_eq!(exhausted.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(exhausted.body.error.code, "fallback_exhausted");

        let unsupported = ApiErrorResponse::from(CoreError::from(
            RoutingError::UnsupportedCapability(Capability::Images),
        ));
        assert_eq!(unsupported.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            unsupported.body.error.code,
            "routing_unsupported_capability"
        );
    }

    #[test]
    fn should_map_provider_errors_to_expected_status_codes() {
        let provider = Provider::Anthropic;
        let cases = [
            (
                ProviderErrorCategory::RateLimit,
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
            ),
            (
                ProviderErrorCategory::ProviderUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_unavailable",
            ),
            (
                ProviderErrorCategory::Timeout,
                StatusCode::GATEWAY_TIMEOUT,
                "request_timeout",
            ),
            (
                ProviderErrorCategory::Unknown,
                StatusCode::BAD_GATEWAY,
                "provider_error",
            ),
            (
                ProviderErrorCategory::MissingCredentials,
                StatusCode::SERVICE_UNAVAILABLE,
                "missing_provider_credentials",
            ),
            (
                ProviderErrorCategory::InvalidCredentials,
                StatusCode::SERVICE_UNAVAILABLE,
                "invalid_provider_credentials",
            ),
            (
                ProviderErrorCategory::Storage,
                StatusCode::BAD_GATEWAY,
                "storage_error",
            ),
            (
                ProviderErrorCategory::ModelNotFound,
                StatusCode::NOT_FOUND,
                "model_not_found",
            ),
            (
                ProviderErrorCategory::Authentication,
                StatusCode::SERVICE_UNAVAILABLE,
                "provider_authentication",
            ),
            (
                ProviderErrorCategory::InvalidRequest,
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_request",
            ),
            (
                ProviderErrorCategory::ContextLength,
                StatusCode::UNPROCESSABLE_ENTITY,
                "context_length_exceeded",
            ),
            (
                ProviderErrorCategory::UnsupportedCapability,
                StatusCode::UNPROCESSABLE_ENTITY,
                "provider_unsupported_capability",
            ),
        ];
        for (category, status, code) in cases {
            let response = ApiErrorResponse::from(CoreError::from(ProviderError {
                category,
                message: "provider said no".to_string(),
                provider: provider.clone(),
                model: Some("claude-sonnet".to_string()),
            }));
            assert_eq!(response.status, status, "status for {category:?}");
            assert_eq!(response.body.error.code, code, "code for {category:?}");
            let details = response.body.error.details.as_ref().unwrap();
            assert_eq!(details["provider"], "anthropic");
            assert_eq!(details["model"], "claude-sonnet");
        }
    }

    #[test]
    fn should_omit_provider_model_when_unknown() {
        let response = ApiErrorResponse::from(CoreError::from(ProviderError {
            category: ProviderErrorCategory::Timeout,
            message: "timeout".to_string(),
            provider: Provider::OpenAI,
            model: None,
        }));
        let details = response.body.error.details.as_ref().unwrap();
        assert_eq!(details["provider"], "openai");
        assert!(details.get("model").is_none());
    }

    #[test]
    fn should_map_internal_error_to_500_without_leaking_message() {
        let sensitive_marker = "leak-marker-9f3a7c";
        let response = ApiErrorResponse::from(CoreError::Internal(format!(
            "downstream call failed: {sensitive_marker}"
        )));
        assert_eq!(response.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(response.body.error.code, "internal_error");
        assert!(response.body.error.details.is_none());
        assert!(!response.body.error.message.contains(sensitive_marker));
    }
}

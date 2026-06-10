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

use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::error::{
    AuthError, CapabilityError, CoreError, ParseError, PolicyError, ProviderError,
    ProviderErrorCategory, RoutingError,
};

/// Envelope wrapping the error body under an `error` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
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
#[ts(export)]
pub struct ApiErrorBody {
    /// Stable machine-readable error code, for example `missing_provider_credentials`.
    pub code: String,
    /// Human-readable, actionable description of what went wrong.
    pub message: String,
    /// Optional structured context; never carries secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
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
            CoreError::Internal(_) => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal error occurred.",
            ),
            CoreError::Parse(error) => map_parse(&error),
            CoreError::Policy(error) => map_policy(&error),
            CoreError::Provider(error) => map_provider(error),
            CoreError::Routing(error) => map_routing(error),
        }
    }
}

fn map_auth(error: &AuthError) -> ApiErrorResponse {
    let message = error.to_string();
    let (status, code) = match error {
        AuthError::Expired => (StatusCode::UNAUTHORIZED, "token_expired"),
        AuthError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
        AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
        AuthError::MissingCredentials => (StatusCode::UNAUTHORIZED, "missing_credentials"),
        AuthError::Revoked => (StatusCode::UNAUTHORIZED, "token_revoked"),
    };
    ApiErrorResponse::new(status, code, message)
}

fn map_capability(error: &CapabilityError) -> ApiErrorResponse {
    let message = error.to_string();
    match error {
        CapabilityError::ContextWindowExceeded {
            estimated_tokens,
            max_context_tokens,
        } => ApiErrorResponse::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "context_window_exceeded",
            message,
        )
        .with_details(serde_json::json!({
            "estimated_tokens": estimated_tokens,
            "max_context_tokens": max_context_tokens,
        })),
        CapabilityError::MissingCapability(capability) => ApiErrorResponse::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "missing_capability",
            message,
        )
        .with_details(serde_json::json!({ "capability": capability })),
    }
}

fn map_parse(error: &ParseError) -> ApiErrorResponse {
    let message = error.to_string();
    let code = match error {
        ParseError::InvalidModelReference(_) => "invalid_model_reference",
        ParseError::UnknownEffort(_) => "unknown_effort",
        ParseError::UnknownIntent(_) => "unknown_intent",
        ParseError::UnknownProvider(_) => "unknown_provider",
    };
    ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
}

fn map_policy(error: &PolicyError) -> ApiErrorResponse {
    let message = error.to_string();
    match error {
        PolicyError::PermissionExpansion {
            base,
            requested,
            tool,
        } => ApiErrorResponse::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "permission_expansion",
            message,
        )
        .with_details(serde_json::json!({
            "tool": tool,
            "base": base,
            "requested": requested,
        })),
    }
}

fn map_routing(error: RoutingError) -> ApiErrorResponse {
    let message = error.to_string();
    match error {
        RoutingError::FallbackExhausted => ApiErrorResponse::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "fallback_exhausted",
            message,
        ),
        RoutingError::NoRoute => {
            ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "no_route", message)
        }
        RoutingError::OverrideNotAllowed(model) => {
            ApiErrorResponse::new(StatusCode::FORBIDDEN, "override_not_allowed", message)
                .with_details(serde_json::json!({ "model": model }))
        }
        RoutingError::UnknownModel(model) => {
            ApiErrorResponse::new(StatusCode::UNPROCESSABLE_ENTITY, "unknown_model", message)
                .with_details(serde_json::json!({ "model": model }))
        }
        RoutingError::UnsupportedCapability(capability) => ApiErrorResponse::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "routing_unsupported_capability",
            message,
        )
        .with_details(serde_json::json!({ "capability": capability })),
    }
}

fn map_provider(error: ProviderError) -> ApiErrorResponse {
    let message = error.to_string();
    let (status, code) = match error.category {
        ProviderErrorCategory::Authentication => {
            (StatusCode::SERVICE_UNAVAILABLE, "provider_authentication")
        }
        ProviderErrorCategory::ContextLength => {
            (StatusCode::UNPROCESSABLE_ENTITY, "context_length_exceeded")
        }
        ProviderErrorCategory::InvalidCredentials => (
            StatusCode::SERVICE_UNAVAILABLE,
            "invalid_provider_credentials",
        ),
        ProviderErrorCategory::InvalidRequest => {
            (StatusCode::UNPROCESSABLE_ENTITY, "invalid_request")
        }
        ProviderErrorCategory::MissingCredentials => (
            StatusCode::SERVICE_UNAVAILABLE,
            "missing_provider_credentials",
        ),
        ProviderErrorCategory::ModelNotFound => (StatusCode::NOT_FOUND, "model_not_found"),
        ProviderErrorCategory::ProviderUnavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable")
        }
        ProviderErrorCategory::RateLimit => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        ProviderErrorCategory::Storage => (StatusCode::INTERNAL_SERVER_ERROR, "storage_error"),
        ProviderErrorCategory::Timeout => (StatusCode::GATEWAY_TIMEOUT, "request_timeout"),
        ProviderErrorCategory::Unknown => (StatusCode::BAD_GATEWAY, "provider_error"),
        ProviderErrorCategory::UnsupportedCapability => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "provider_unsupported_capability",
        ),
    };
    let mut details = serde_json::json!({ "provider": error.provider });
    if let Some(model) = error.model {
        details["model"] = serde_json::Value::String(model);
    }
    ApiErrorResponse::new(status, code, message).with_details(details)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Capability, Provider};
    use crate::policy::PermissionMode;

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
                StatusCode::INTERNAL_SERVER_ERROR,
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
                provider,
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

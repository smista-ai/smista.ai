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

use serde::{Deserialize, Serialize};

/// Envelope wrapping the error body under an `error` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// The structured error payload.
    pub error: ApiErrorBody,
}

/// The structured payload of an [`ApiError`].
///
/// `details` carries machine-readable context (such as the offending provider
/// and model) and is omitted when there is none. It must never contain secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    /// Stable machine-readable error code, for example `missing_provider_credentials`.
    pub code: String,
    /// Human-readable, actionable description of what went wrong.
    pub message: String,
    /// Optional structured context; never carries secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

//! Authentication request and response bodies for `/auth`.
//!
//! These types cover the authentication lifecycle: bootstrapping a user and its
//! long-lived API key, exchanging that key for a short-lived session token,
//! revoking the token, and listing the authenticated user's sessions.
//!
//! Two fields are secrets and appear exactly once: [`BootstrapResponse::api_key`]
//! (returned only at user creation) and [`SignInResponse::token`]. They must
//! never be logged, traced, or persisted unredacted.
//!

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Placeholder rendered in place of a secret field by `Debug`.
const REDACTED: &str = "<redacted>";

/// Response to `POST /auth/bootstrap`, returned once at user creation.
///
/// `api_key` is a secret shown only here; the caller must store it securely.
/// `Debug` is implemented by hand to redact `api_key`, so the credential is
/// never leaked through `{:?}` formatting, logs, or traces.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct BootstrapResponse {
    /// Identifier of the created user, for example `user:abc123`.
    pub user_id: String,
    /// Long-lived smista API key. Secret; shown only in this response.
    pub api_key: String,
}

impl fmt::Debug for BootstrapResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BootstrapResponse")
            .field("user_id", &self.user_id)
            .field("api_key", &REDACTED)
            .finish()
    }
}

/// Response to `POST /auth/sign-in`, carrying the session token.
///
/// `token` is a secret bearer credential; `expires_at` is when it stops being
/// valid. `Debug` is implemented by hand to redact `token`, so the credential
/// is never leaked through `{:?}` formatting, logs, or traces.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct SignInResponse {
    /// Session bearer token. Secret; sent as `Authorization: Bearer`.
    pub token: String,
    /// Instant at which the token expires.
    pub expires_at: DateTime<Utc>,
}

impl fmt::Debug for SignInResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignInResponse")
            .field("token", &REDACTED)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Response to `POST /auth/sign-out`, confirming token revocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct SignOutResponse {
    /// Whether the session token was revoked.
    pub revoked: bool,
}

/// Response to `GET /auth/me`, identifying the authenticated user.
///
/// Carries only the caller's user ID; it lists no sessions and exposes no
/// secret values. To enumerate sessions, use `GET /sessions` instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[ts(export)]
pub struct MeResponse {
    /// The authenticated user's unique identifier.
    pub user_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_bootstrap_to_spec_shape() {
        let response = BootstrapResponse {
            user_id: "user:abc123".to_string(),
            api_key: "sk-smista-api01-...".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            serde_json::json!({
                "user_id": "user:abc123",
                "api_key": "sk-smista-api01-...",
            })
        );
    }

    #[test]
    fn should_deserialize_sign_in_response_from_spec() {
        let json = r#"{"token":"st_...","expires_at":"2026-05-25T12:00:00Z"}"#;
        let response: SignInResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.token, "st_...");
        assert_eq!(
            response.expires_at,
            "2026-05-25T12:00:00Z".parse::<DateTime<Utc>>().unwrap()
        );
    }

    #[test]
    fn should_redact_api_key_in_debug() {
        let response = BootstrapResponse {
            user_id: "user:abc123".to_string(),
            api_key: "sk-smista-api01-secret".to_string(),
        };
        let debug = format!("{response:?}");
        assert!(!debug.contains("sk-smista-api01-secret"));
        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("user:abc123"));
    }

    #[test]
    fn should_redact_token_in_debug() {
        let response = SignInResponse {
            token: "st_secret_token".to_string(),
            expires_at: "2026-05-25T12:00:00Z".parse::<DateTime<Utc>>().unwrap(),
        };
        let debug = format!("{response:?}");
        assert!(!debug.contains("st_secret_token"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn should_serialize_sign_out_response() {
        assert_eq!(
            serde_json::to_value(SignOutResponse { revoked: true }).unwrap(),
            serde_json::json!({ "revoked": true })
        );
    }

    #[test]
    fn should_serialize_me_response_to_spec_shape() {
        assert_eq!(
            serde_json::to_value(MeResponse {
                user_id: "018f9c3e-7a2b-7c4d-8e5f-1a2b3c4d5e6f".to_string(),
            })
            .unwrap(),
            serde_json::json!({ "user_id": "018f9c3e-7a2b-7c4d-8e5f-1a2b3c4d5e6f" })
        );
    }
}

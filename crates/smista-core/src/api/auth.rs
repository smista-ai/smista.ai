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
//! # Examples
//!
//! ```
//! use smista_core::api::SignInRequest;
//!
//! let request = SignInRequest { user_id: "user:abc123".to_string() };
//! let json = serde_json::to_string(&request).unwrap();
//! assert_eq!(json, r#"{"user_id":"user:abc123"}"#);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::session::SessionSummary;

/// Response to `POST /auth/bootstrap`, returned once at user creation.
///
/// `api_key` is a secret shown only here; the caller must store it securely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct BootstrapResponse {
    /// Identifier of the created user, for example `user:abc123`.
    pub user_id: String,
    /// Long-lived smista API key. Secret; shown only in this response.
    pub api_key: String,
}

/// Body of `POST /auth/sign-in`, naming the user to authenticate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SignInRequest {
    /// Identifier of the user signing in.
    pub user_id: String,
}

/// Response to `POST /auth/sign-in`, carrying the session token.
///
/// `token` is a secret bearer credential; `expires_at` is when it stops being
/// valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SignInResponse {
    /// Session bearer token. Secret; sent as `Authorization: Bearer`.
    pub token: String,
    /// Instant at which the token expires.
    pub expires_at: DateTime<Utc>,
}

/// Response to `POST /auth/sign-out`, confirming token revocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct SignOutResponse {
    /// Whether the session token was revoked.
    pub revoked: bool,
}

/// Response to `GET /auth/me`, listing the authenticated user's sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct MeResponse {
    /// Summaries of the user's sessions.
    pub sessions: Vec<SessionSummary>,
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
    fn should_roundtrip_sign_in_request() {
        let request = SignInRequest {
            user_id: "user:abc123".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<SignInRequest>(&json).unwrap(),
            request
        );
    }

    #[test]
    fn should_serialize_sign_out_response() {
        assert_eq!(
            serde_json::to_value(SignOutResponse { revoked: true }).unwrap(),
            serde_json::json!({ "revoked": true })
        );
    }

    #[test]
    fn should_serialize_empty_me_response() {
        assert_eq!(
            serde_json::to_value(MeResponse {
                sessions: Vec::new()
            })
            .unwrap(),
            serde_json::json!({ "sessions": [] })
        );
    }
}

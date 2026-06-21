//! Response body for the public `GET /status` health check.
//!
//! This endpoint sits at the service root, outside `/api/v1`, and requires no
//! authentication. It reports that the service is up together with its running
//! version.
//!
//! # Examples
//!
//! ```
//! use smista_core::api::StatusResponse;
//!
//! let response = StatusResponse {
//!     status: "ok".to_string(),
//!     version: "1.2.3".to_string(),
//! };
//! let json = serde_json::to_string(&response).unwrap();
//! assert_eq!(json, r#"{"status":"ok","version":"1.2.3"}"#);
//! ```

use serde::{Deserialize, Serialize};

/// Body of a successful `GET /status` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "ts", ts(export))]
pub struct StatusResponse {
    /// Service liveness indicator; `"ok"` when the server answers.
    pub status: String,
    /// The running service version.
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_status_to_spec_shape() {
        let response = StatusResponse {
            status: "ok".to_string(),
            version: "1.2.3".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            serde_json::json!({
                "status": "ok",
                "version": "1.2.3",
            })
        );
    }

    #[test]
    fn should_roundtrip_status_response() {
        let json = r#"{"status":"ok","version":"1.2.3"}"#;
        let response: StatusResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response,
            StatusResponse {
                status: "ok".to_string(),
                version: "1.2.3".to_string(),
            }
        );
    }
}

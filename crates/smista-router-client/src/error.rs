//! Errors returned by a [`Client`](crate::Client).
//!
//! Every fallible client method returns [`RouterClientError`]. It separates the
//! cases a caller acts on differently: a structured [`Api`](RouterClientError::Api)
//! error decoded from the router's error body and paired with the HTTP status, a
//! [`Decode`](RouterClientError::Decode) failure on a malformed body, a
//! [`NotAuthenticated`](RouterClientError::NotAuthenticated) state before a token
//! is held, a backend-agnostic [`Transport`](RouterClientError::Transport)
//! failure, and a bad [`Url`](RouterClientError::Url).
//!
//! The trait crate depends on no HTTP library, so `Transport` carries an erased
//! [`std::error::Error`] rather than a concrete client's transport type; each
//! concrete client maps its own failure into it through
//! [`RouterClientError::transport`].
//!
//! No variant carries a credential: the API error body is redacted by the router
//! before it is sent, and neither the API key, the session token nor any provider
//! credential is ever placed in an error.

use smista_core::api::ApiError;

/// A specialized [`Result`](std::result::Result) for client operations.
pub type Result<T> = std::result::Result<T, RouterClientError>;

/// The error returned by every fallible [`Client`](crate::Client) method.
///
/// Variants are ordered alphabetically by name. The enum is `#[non_exhaustive]`
/// so concrete clients and future revisions can add cases without a breaking
/// change; match with a wildcard arm.
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum RouterClientError {
    /// The router returned a structured error body with a non-success status.
    ///
    /// `code` is the stable, machine-readable
    /// [`ApiErrorCode`](smista_core::api::ApiErrorCode) string a caller matches
    /// on; `status` is the HTTP status that accompanied it; `details` is the
    /// optional machine-readable context, which never contains secrets.
    #[error("API error (status {status}, code {code}): {message}")]
    Api {
        /// HTTP status code that accompanied the error body.
        status: u16,
        /// Stable, machine-readable error code, for example `invalid_token`.
        code: String,
        /// Human-readable description of what went wrong.
        message: String,
        /// Optional machine-readable context; never carries secrets.
        details: Option<serde_json::Value>,
    },
    /// A response body could not be decoded into its expected type.
    #[error("failed to decode response body: {0}")]
    Decode(#[from] serde_json::Error),
    /// An authenticated method was called before the client held a session
    /// token. Call `sign_in` (or `bootstrap` then `sign_in`) first.
    #[error("client is not authenticated; sign in first")]
    NotAuthenticated,
    /// The underlying HTTP backend failed to complete the request, for example
    /// because the connection was refused or timed out.
    #[error("transport error: {0}")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A request URL could not be built from the configured base URL.
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
}

impl RouterClientError {
    /// Builds an [`Api`](Self::Api) error from an HTTP `status` and the router's
    /// decoded [`ApiError`] body.
    ///
    /// This is the single place a concrete client turns a decoded error body
    /// into a [`RouterClientError`], so the mapping stays consistent across
    /// backends.
    #[must_use]
    pub fn api(status: u16, error: ApiError) -> Self {
        Self::Api {
            status,
            code: error.error.code,
            message: error.error.message,
            details: error.error.details,
        }
    }

    /// Wraps a concrete client's transport failure as a
    /// [`Transport`](Self::Transport) error.
    ///
    /// The trait crate is HTTP-library agnostic, so a backend boxes its own
    /// error type into the erased source this carries.
    #[must_use]
    pub fn transport<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Transport(Box::new(source))
    }
}

#[cfg(test)]
mod tests {
    use smista_core::api::ApiErrorBody;

    use super::*;

    #[test]
    fn should_build_api_error_from_decoded_body() {
        let error = RouterClientError::api(
            401,
            ApiError {
                error: ApiErrorBody {
                    code: "invalid_token".to_string(),
                    message: "The session token is malformed or unknown.".to_string(),
                    details: None,
                },
            },
        );
        let RouterClientError::Api {
            status,
            code,
            message,
            details,
        } = &error
        else {
            panic!("expected an API error");
        };
        assert_eq!(*status, 401);
        assert_eq!(code, "invalid_token");
        assert_eq!(message, "The session token is malformed or unknown.");
        assert!(details.is_none());
    }

    #[test]
    fn should_render_api_error_with_status_and_code() {
        let error = RouterClientError::api(
            404,
            ApiError {
                error: ApiErrorBody {
                    code: "session_not_found".to_string(),
                    message: "Session not found.".to_string(),
                    details: None,
                },
            },
        );
        assert_eq!(
            error.to_string(),
            "API error (status 404, code session_not_found): Session not found."
        );
    }

    #[test]
    fn should_wrap_a_transport_source() {
        let io = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let error = RouterClientError::transport(io);
        assert!(matches!(error, RouterClientError::Transport(_)));
        assert!(error.to_string().starts_with("transport error: "));
        // The boxed cause is exposed as the error source.
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn should_convert_a_decode_failure() {
        let decode = serde_json::from_str::<smista_core::api::StatusResponse>("not json")
            .expect_err("malformed JSON should not decode");
        let error = RouterClientError::from(decode);
        assert!(matches!(error, RouterClientError::Decode(_)));
        assert!(
            error
                .to_string()
                .starts_with("failed to decode response body")
        );
    }

    #[test]
    fn should_convert_a_url_parse_failure() {
        let parse = "::not a url"
            .parse::<url::Url>()
            .expect_err("should not parse");
        let error = RouterClientError::from(parse);
        assert!(matches!(error, RouterClientError::Url(_)));
        assert!(error.to_string().starts_with("invalid URL: "));
    }

    #[test]
    fn should_describe_the_unauthenticated_state() {
        assert_eq!(
            RouterClientError::NotAuthenticated.to_string(),
            "client is not authenticated; sign in first"
        );
    }
}

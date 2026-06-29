//! The orchestrator's error type and its mapping to stable API error codes.
//!
//! Every failure the run loop can surface is one [`OrchestratorError`] variant.
//! [`OrchestratorError::api_code`] maps each to the stable
//! [`ApiErrorCode`](smista_core::api::ApiErrorCode) the client matches on, and a
//! [`From`] conversion turns it into the HTTP [`WebError`].
use smista_core::api::ApiErrorCode;
use smista_core::error::{ProviderError, ProviderErrorCategory};

use crate::orchestrator::crypto::CryptoMapError;
use crate::router::resolver::ResolverError;
use crate::session::SessionError;
use crate::web::error::WebError;

/// A failure surfaced while driving a run.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OrchestratorError {
    /// A turn is already in flight for the session; the run cannot be admitted
    /// until it reaches a checkpoint.
    #[error("a turn is already in flight for this session")]
    Busy,
    /// A content reference could not be mapped to a storage row.
    #[error("crypto mapping error: {0}")]
    Crypto(#[from] CryptoMapError),
    /// The primary model and every fallback failed.
    #[error("the primary route and every fallback failed")]
    FallbackExhausted,
    /// An unexpected internal condition the client cannot act on; the message is
    /// for logs only and must never carry secrets.
    #[error("internal error: {0}")]
    Internal(String),
    /// A continuation arrived for a session with no run to advance.
    #[error("no run is in progress for this session")]
    NoActiveRun,
    /// A provider rejected or failed the invocation.
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    /// The deterministic resolver could not resolve the turn.
    #[error("resolver error: {0}")]
    Resolver(#[from] ResolverError),
    /// A session read or write failed.
    #[error("session error: {0}")]
    Session(#[from] SessionError),
    /// The in-flight turn was cancelled by a newer request.
    #[error("the turn was superseded by a newer request")]
    Superseded,
    /// The continuation does not answer the run's current checkpoint.
    #[error("the continuation does not answer the current pause")]
    UnexpectedContinuation,
}

impl OrchestratorError {
    /// Whether this failure must leave the run's durable checkpoint intact.
    ///
    /// A rejected continuation (the client answered the wrong pause, named an
    /// unknown call, or sent an incomplete set of tool results) is the client's
    /// mistake, not the run's: the pause it failed to answer is still valid, so
    /// the lock is released back to that same checkpoint instead of being reset
    /// to idle. Every other failure rolls the run back to idle.
    pub(crate) fn preserves_checkpoint(&self) -> bool {
        matches!(self, Self::UnexpectedContinuation)
    }

    /// Maps the error onto the stable [`ApiErrorCode`] clients match on.
    pub(crate) fn api_code(&self) -> ApiErrorCode {
        match self {
            Self::Busy => ApiErrorCode::RunInFlight,
            Self::Session(SessionError::NotFound) => ApiErrorCode::SessionNotFound,
            Self::Crypto(_) | Self::Internal(_) | Self::Session(_) | Self::Superseded => {
                ApiErrorCode::InternalError
            }
            Self::FallbackExhausted => ApiErrorCode::FallbackExhausted,
            Self::NoActiveRun | Self::UnexpectedContinuation => ApiErrorCode::InvalidRequest,
            Self::Provider(error) => provider_api_code(error.category),
            // The resolver owns the canonical mapping for its own failures.
            Self::Resolver(error) => error.api_code(),
        }
    }
}

/// Maps a provider error category onto the stable API error code.
fn provider_api_code(category: ProviderErrorCategory) -> ApiErrorCode {
    match category {
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
        ProviderErrorCategory::UnsupportedCapability => ApiErrorCode::ProviderUnsupportedCapability,
        ProviderErrorCategory::Unknown => ApiErrorCode::ProviderError,
    }
}

impl From<OrchestratorError> for WebError {
    fn from(error: OrchestratorError) -> Self {
        let code = error.api_code();
        WebError::from_code(code, error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_map_busy_to_run_in_flight() {
        assert_eq!(
            OrchestratorError::Busy.api_code(),
            ApiErrorCode::RunInFlight
        );
    }

    #[test]
    fn should_map_fallback_exhausted_to_api_code() {
        assert_eq!(
            OrchestratorError::FallbackExhausted.api_code(),
            ApiErrorCode::FallbackExhausted
        );
    }

    #[test]
    fn should_map_a_not_found_session_to_session_not_found() {
        let error = OrchestratorError::Session(SessionError::NotFound);
        assert_eq!(error.api_code(), ApiErrorCode::SessionNotFound);
    }

    #[test]
    fn should_delegate_resolver_errors_to_the_resolver_mapping() {
        use crate::router::resolver::policy_matcher::PolicyMatchError;

        // The resolver owns the canonical code; the orchestrator forwards it
        // verbatim rather than re-deriving it.
        let error = OrchestratorError::Resolver(ResolverError::Routing(PolicyMatchError::NoRoute));
        assert_eq!(error.api_code(), ApiErrorCode::NoRoute);
    }
}

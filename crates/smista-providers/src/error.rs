//! Shared mapping from `rig`, `reqwest` and serialization errors into the
//! crate-agnostic [`ProviderError`].
//!
//! This is the single source of truth for the *condition → category* table
//! described in the provider error-handling spec. Every adapter routes its
//! failures through these helpers and never constructs a
//! [`ProviderErrorCategory`] ad hoc, so fallback eligibility (decided in core
//! via [`ProviderErrorCategory::is_fallback_eligible`]) stays consistent across
//! providers.
//!
//! `rig` has no single error type: a call can fail with a [`CompletionError`], a
//! transport-level [`RigHttpClientError`], a [`reqwest::Error`], or a
//! [`serde_json::Error`] while (de)serializing payloads. Each gets a
//! `category_from_*` classifier.
//!
//! Two facts shape the heuristics:
//!
//! - There is no dedicated `Timeout` or `RateLimit` source variant. Timeouts
//!   and connection failures surface as a [`reqwest::Error`] (sometimes boxed
//!   inside [`RigHttpClientError::Instance`] or
//!   [`CompletionError::RequestError`]) and are recovered with
//!   [`reqwest::Error::is_timeout`] / [`reqwest::Error::is_connect`].
//! - Most providers report rate limits, missing models, quota rejections and
//!   context-length overflows inside a free-form message rather than a clean
//!   HTTP status, so those are recovered with substring heuristics.
//!
//! The `message` stored on a [`ProviderError`] is always run through
//! [`redact`], which scrubs API keys and bearer tokens before the error is
//! constructed, upholding the secrecy invariant.
//!

use rig_core::completion::request::CompletionError;
use rig_core::http_client::Error as RigHttpClientError;
use smista_core::error::{ProviderError, ProviderErrorCategory};
use smista_core::model::Provider;

/// Placeholder substituted for any secret scrubbed from an error message.
const REDACTED: &str = "[redacted]";

/// Token prefixes that mark a value as a provider credential to be redacted.
///
/// Covers OpenAI/Anthropic (`sk-`, including `sk-ant-`/`sk-proj-`), xAI
/// (`xai-`) and GitHub (`ghp_`, `gho_`, `github_pat_`) secrets. A prefix only
/// triggers redaction at a token boundary, so ordinary words that merely embed
/// these letters (for example `disk-space`) are left intact.
const SECRET_PREFIXES: &[&str] = &["sk-", "xai-", "ghp_", "gho_", "github_pat_"];

/// Substrings indicating the request exceeded the model context window.
const CONTEXT_LENGTH_MARKERS: &[&str] = &[
    "context length",
    "context window",
    "maximum context",
    "maximum tokens",
    "too many tokens",
];

/// Substrings indicating the referenced model is not offered by the provider.
const MODEL_NOT_FOUND_MARKERS: &[&str] = &[
    "model not found",
    "model_not_found",
    "does not exist",
    "no such model",
    "unknown model",
];

/// Substrings indicating the provider rate-limited the request.
const RATE_LIMIT_MARKERS: &[&str] = &["rate limit", "rate_limit", "too many requests"];

/// Substrings indicating an authenticated request was rejected by policy.
///
/// These describe quota, billing or plan-level rejections — the request was
/// authenticated, but the account is not allowed to make it — which map to
/// [`ProviderErrorCategory::Authentication`] rather than the transient
/// [`ProviderErrorCategory::RateLimit`].
const AUTHENTICATION_MARKERS: &[&str] = &[
    "insufficient_quota",
    "quota exceeded",
    "exceeded your current quota",
    "billing",
];

/// Substrings indicating the configured credentials were rejected.
const INVALID_CREDENTIALS_MARKERS: &[&str] = &[
    "invalid api key",
    "incorrect api key",
    "invalid_api_key",
    "invalid authentication",
    "no api key",
];

/// Substrings indicating the call timed out before a response arrived.
const TIMEOUT_MARKERS: &[&str] = &["timed out", "timeout", "deadline exceeded"];

/// Builds a [`ProviderError`] with a redacted, provider-agnostic message.
///
/// The `category` is supplied by a `category_from_*` classifier (or directly by
/// an adapter for conditions it detects itself, such as
/// [`ProviderErrorCategory::MissingCredentials`]). `provider` and the optional
/// `model` are recorded for diagnostics. `message` is scrubbed through
/// [`redact`] so credentials never reach logs or API clients.
pub(crate) fn provider_error(
    category: ProviderErrorCategory,
    provider: Provider,
    model: Option<String>,
    message: impl Into<String>,
) -> ProviderError {
    ProviderError {
        category,
        message: redact(&message.into()),
        provider,
        model,
    }
}

/// Classifies a [`reqwest::Error`] from direct HTTP transport.
///
/// Timeouts and connection failures are recognized through `reqwest`'s own
/// predicates; otherwise an HTTP status, when present, is delegated to core's
/// `From<StatusCode>`. Unrecognized transport errors fall back to
/// [`ProviderErrorCategory::Unknown`].
pub(crate) fn category_from_reqwest(error: &reqwest::Error) -> ProviderErrorCategory {
    if error.is_timeout() {
        ProviderErrorCategory::Timeout
    } else if error.is_connect() {
        ProviderErrorCategory::ProviderUnavailable
    } else if let Some(status) = error.status() {
        status.into()
    } else {
        ProviderErrorCategory::Unknown
    }
}

/// Classifies a [`serde_json::Error`] from a malformed request or response body.
///
/// A (de)serialization failure means the body did not match the expected shape,
/// which maps to [`ProviderErrorCategory::InvalidRequest`].
pub(crate) fn category_from_serde(_error: &serde_json::Error) -> ProviderErrorCategory {
    ProviderErrorCategory::InvalidRequest
}

/// Classifies a [`CompletionError`], the root of `rig`'s provider failures.
pub(crate) fn category_from_completion(error: &CompletionError) -> ProviderErrorCategory {
    match error {
        CompletionError::HttpError(http) => category_from_http(http),
        // A malformed URL or body is a request-construction problem.
        CompletionError::UrlError(_) | CompletionError::JsonError(_) => {
            ProviderErrorCategory::InvalidRequest
        }
        CompletionError::RequestError(source) => category_from_source(source.as_ref()),
        // A failed response parse points at a malformed body unless the message
        // reveals a more specific provider condition.
        CompletionError::ResponseError(message) => {
            message_or(message, ProviderErrorCategory::InvalidRequest)
        }
        // An opaque provider-reported error: lean on message heuristics.
        CompletionError::ProviderError(message) => {
            message_or(message, ProviderErrorCategory::Unknown)
        }
        CompletionError::ProviderResponse(response) => {
            let fallback = response
                .status
                .map_or(ProviderErrorCategory::Unknown, Into::into);
            message_or(&response.body, fallback)
        }
        _ => ProviderErrorCategory::Unknown,
    }
}

/// Classifies a transport-level [`RigHttpClientError`].
pub(crate) fn category_from_http(error: &RigHttpClientError) -> ProviderErrorCategory {
    match error {
        RigHttpClientError::InvalidStatusCode(status) => {
            let category: ProviderErrorCategory = (*status).into();
            category
        }
        RigHttpClientError::InvalidStatusCodeWithMessage(status, message) => {
            let base: ProviderErrorCategory = (*status).into();
            message_or(message, base)
        }
        RigHttpClientError::Instance(source) => category_from_source(source.as_ref()),
        // Header, content-type and stream errors are not provider conditions we
        // can act on.
        _ => ProviderErrorCategory::Unknown,
    }
}

/// Classifies a boxed transport error, recovering `reqwest` failures.
///
/// `rig` boxes the underlying transport error, so its kind (timeout, connect
/// failure, HTTP status) is only recoverable by downcasting to
/// [`reqwest::Error`]. Anything else is classified by its message.
fn category_from_source(source: &(dyn std::error::Error + 'static)) -> ProviderErrorCategory {
    source.downcast_ref::<reqwest::Error>().map_or_else(
        || categorize_message(&source.to_string()),
        category_from_reqwest,
    )
}

/// Recovers a provider category from a free-form error message.
///
/// Providers commonly report rate limits, missing models, quota rejections and
/// context-length overflows in the message body rather than as a clean HTTP
/// status. Checks run most-specific first; an unmatched message yields
/// [`ProviderErrorCategory::Unknown`].
fn categorize_message(message: &str) -> ProviderErrorCategory {
    let message = message.to_ascii_lowercase();
    let contains = |markers: &[&str]| markers.iter().any(|marker| message.contains(marker));

    if contains(CONTEXT_LENGTH_MARKERS) {
        ProviderErrorCategory::ContextLength
    } else if contains(MODEL_NOT_FOUND_MARKERS) {
        ProviderErrorCategory::ModelNotFound
    } else if contains(RATE_LIMIT_MARKERS) {
        ProviderErrorCategory::RateLimit
    } else if contains(AUTHENTICATION_MARKERS) {
        ProviderErrorCategory::Authentication
    } else if contains(INVALID_CREDENTIALS_MARKERS) {
        ProviderErrorCategory::InvalidCredentials
    } else if contains(TIMEOUT_MARKERS) {
        ProviderErrorCategory::Timeout
    } else {
        ProviderErrorCategory::Unknown
    }
}

/// Returns the message-derived category, or `fallback` when none matches.
///
/// HTTP statuses cannot express categories like [`ContextLength`] or
/// [`ModelNotFound`]; when the message reveals one it takes precedence over the
/// status- or context-derived `fallback`.
///
/// [`ContextLength`]: ProviderErrorCategory::ContextLength
/// [`ModelNotFound`]: ProviderErrorCategory::ModelNotFound
fn message_or(message: &str, fallback: ProviderErrorCategory) -> ProviderErrorCategory {
    match categorize_message(message) {
        ProviderErrorCategory::Unknown => fallback,
        category => category,
    }
}

/// Scrubs API keys and bearer tokens from a free-form message.
///
/// Tokens beginning with a known credential prefix (see [`SECRET_PREFIXES`]),
/// and the value following a `Bearer` keyword, are replaced with [`REDACTED`].
/// This upholds the secrecy invariant on [`ProviderError::message`]: an upstream
/// error string that echoes the request's `Authorization` header must not leak
/// the credential. Whitespace is normalized to single spaces.
fn redact(message: &str) -> String {
    let mut redacted = Vec::new();
    let mut after_bearer = false;
    for token in message.split_whitespace() {
        if after_bearer {
            // The token following a `Bearer` keyword is the credential itself.
            redacted.push(REDACTED);
            after_bearer = false;
            continue;
        }
        if token.eq_ignore_ascii_case("bearer") {
            redacted.push(token);
            after_bearer = true;
        } else if has_secret(token) {
            redacted.push(REDACTED);
        } else {
            redacted.push(token);
        }
    }
    redacted.join(" ")
}

/// Returns `true` when `token` embeds a credential at a token boundary.
fn has_secret(token: &str) -> bool {
    SECRET_PREFIXES.iter().any(|prefix| {
        token.match_indices(prefix).any(|(index, _)| {
            let at_boundary = index == 0
                || matches!(
                    token.as_bytes()[index - 1],
                    b'=' | b':' | b'"' | b'\'' | b'`' | b'(' | b'[' | b'{' | b','
                );
            // Require content after the prefix so a bare prefix is not redacted.
            at_boundary && index + prefix.len() < token.len()
        })
    })
}

#[cfg(test)]
mod test {
    use super::*;

    fn json_error() -> serde_json::Error {
        serde_json::from_str::<i32>("not a number").unwrap_err()
    }

    #[tokio::test]
    async fn should_map_reqwest_connect_failure_to_provider_unavailable() {
        // Port 1 on loopback refuses connections, yielding a deterministic
        // connect error without any external network access.
        let error = reqwest::Client::new()
            .get("http://127.0.0.1:1/")
            .send()
            .await
            .expect_err("connecting to a closed loopback port must fail");

        assert!(error.is_connect());
        assert_eq!(
            category_from_reqwest(&error),
            ProviderErrorCategory::ProviderUnavailable
        );
    }

    #[test]
    fn should_map_unauthorized_status_to_invalid_credentials() {
        let error = CompletionError::HttpError(RigHttpClientError::InvalidStatusCode(
            reqwest::StatusCode::UNAUTHORIZED,
        ));

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::InvalidCredentials
        );
    }

    #[test]
    fn should_map_server_error_status_to_provider_unavailable() {
        let error =
            RigHttpClientError::InvalidStatusCode(reqwest::StatusCode::INTERNAL_SERVER_ERROR);

        assert_eq!(
            category_from_http(&error),
            ProviderErrorCategory::ProviderUnavailable
        );
    }

    #[test]
    fn should_refine_bad_request_with_context_length_message() {
        let error = CompletionError::HttpError(RigHttpClientError::InvalidStatusCodeWithMessage(
            reqwest::StatusCode::BAD_REQUEST,
            "This model's maximum context length is 8192 tokens".to_string(),
        ));

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::ContextLength
        );
    }

    #[test]
    fn should_map_provider_error_model_not_found_message() {
        let error = CompletionError::ProviderError("The model `gpt-9` does not exist".to_string());

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::ModelNotFound
        );
    }

    #[test]
    fn should_map_provider_error_rate_limit_message() {
        let error = CompletionError::ProviderError("Rate limit reached for requests".to_string());

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::RateLimit
        );
    }

    #[test]
    fn should_map_provider_response_status() {
        let error = CompletionError::ProviderResponse(rig_core::ProviderResponseError {
            status: Some(reqwest::StatusCode::SERVICE_UNAVAILABLE),
            body: "temporarily unavailable".to_string(),
        });

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::ProviderUnavailable
        );
    }

    #[test]
    fn should_refine_provider_response_status_with_body() {
        let error = CompletionError::ProviderResponse(rig_core::ProviderResponseError {
            status: Some(reqwest::StatusCode::BAD_REQUEST),
            body: "This model's maximum context length is 8192 tokens".to_string(),
        });

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::ContextLength
        );
    }

    #[test]
    fn should_map_provider_response_without_status_from_body() {
        let error = CompletionError::ProviderResponse(rig_core::ProviderResponseError {
            status: None,
            body: "Rate limit reached for requests".to_string(),
        });

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::RateLimit
        );
    }

    #[test]
    fn should_map_quota_message_to_authentication() {
        let error = CompletionError::ProviderError("You exceeded your current quota".to_string());

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::Authentication
        );
    }

    #[test]
    fn should_map_url_error_to_invalid_request() {
        let error = CompletionError::UrlError("not a url".parse::<url::Url>().unwrap_err());

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::InvalidRequest
        );
    }

    #[test]
    fn should_map_json_completion_error_to_invalid_request() {
        let error = CompletionError::JsonError(json_error());

        assert_eq!(
            category_from_completion(&error),
            ProviderErrorCategory::InvalidRequest
        );
    }

    #[test]
    fn should_map_bare_serde_error_to_invalid_request() {
        assert_eq!(
            category_from_serde(&json_error()),
            ProviderErrorCategory::InvalidRequest
        );
    }

    #[test]
    fn should_build_provider_error_with_category_and_context() {
        let error = provider_error(
            ProviderErrorCategory::RateLimit,
            Provider::OpenAI,
            Some("gpt-4".to_string()),
            "rate limit reached",
        );

        assert_eq!(error.category, ProviderErrorCategory::RateLimit);
        assert_eq!(error.provider, Provider::OpenAI);
        assert_eq!(error.model.as_deref(), Some("gpt-4"));
        assert!(error.message.contains("rate limit reached"));
    }

    #[test]
    fn should_redact_api_key_from_message() {
        let error = provider_error(
            ProviderErrorCategory::InvalidCredentials,
            Provider::Anthropic,
            None,
            "request failed with key sk-ant-api03-TOPSECRETVALUE rejected",
        );

        assert!(!error.message.contains("sk-ant-api03-TOPSECRETVALUE"));
        assert!(error.message.contains(REDACTED));
    }

    #[test]
    fn should_redact_bearer_token_from_message() {
        let redacted = redact("Authorization: Bearer abc123.def456.ghi789 was invalid");

        assert!(!redacted.contains("abc123.def456.ghi789"));
        assert!(redacted.contains(REDACTED));
    }

    #[test]
    fn should_not_redact_ordinary_words_resembling_prefixes() {
        let redacted = redact("the disk-space ran out while writing the task-list");

        assert_eq!(
            redacted,
            "the disk-space ran out while writing the task-list"
        );
    }
}

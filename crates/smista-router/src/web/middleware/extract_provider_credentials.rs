//! Provider credential extraction.
//!
//! [`extract_provider_credentials`] lifts any
//! `X-Smista-Provider-<provider>-Api-Key` header into a [`RequestCredentials`]
//! map stored in the request extensions, so a handler can use the caller's own
//! provider key for that single request. The keys are kept in [`SecretString`]
//! and are never logged, traced or persisted.

use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::str::FromStr as _;

use axum::extract::Request;
use axum::http::header::HeaderName;
use axum::middleware::Next;
use axum::response::Response;
use secrecy::SecretString;
use smista_core::model::Provider;

/// Lowercase header-name prefix that marks a per-request provider credential.
///
/// A provider credential travels as `X-Smista-Provider-<provider>-Api-Key`.
/// HTTP header names are case-insensitive and `axum` lowercases them, so the
/// prefix and [`PROVIDER_CREDENTIAL_SUFFIX`] are matched in lowercase.
const PROVIDER_CREDENTIAL_PREFIX: &str = "x-smista-provider-";

/// Lowercase header-name suffix that closes a provider credential header. See
/// [`PROVIDER_CREDENTIAL_PREFIX`].
const PROVIDER_CREDENTIAL_SUFFIX: &str = "-api-key";

/// Provider credentials supplied by the caller through request headers, scoped
/// to a single request.
///
/// Populated by [`extract_provider_credentials`] and read by handlers through
/// the [`Extension`](axum::Extension) extractor. The keys are [`SecretString`]s,
/// so they are redacted from `Debug` output and must never be logged, traced or
/// persisted.
#[derive(Clone, Default)]
pub(crate) struct RequestCredentials(HashMap<Provider, SecretString>);

impl RequestCredentials {
    /// Records the API key the caller supplied for `provider`.
    fn insert(&mut self, provider: Provider, api_key: SecretString) {
        self.0.insert(provider, api_key);
    }

    /// Returns the full map of provider credentials.
    pub(crate) fn credentials(self) -> HashMap<Provider, SecretString> {
        self.0
    }

    /// Returns the API key the caller supplied for `provider`, if any.
    #[cfg(test)]
    pub(crate) fn get(&self, provider: &Provider) -> Option<&SecretString> {
        self.0.get(provider)
    }

    /// Returns whether the caller supplied any provider credential.
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

// Implemented by hand rather than derived so the secret values are never
// rendered, even as the set of providers grows.
impl Debug for RequestCredentials {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestCredentials")
            // Provider names are not sensitive; the API keys are omitted.
            .field("providers", &self.0.keys())
            .finish()
    }
}

/// Extracts per-request provider credentials from the request headers and
/// stashes them in the request extensions for the handler to read.
///
/// Each `X-Smista-Provider-<provider>-Api-Key` header contributes one entry to
/// a [`RequestCredentials`] map, which is inserted only when at least one
/// credential is present. The credentials live for this request alone: they are
/// dropped with the request and are never logged, traced or persisted.
pub(crate) async fn extract_provider_credentials(mut request: Request, next: Next) -> Response {
    let mut credentials = RequestCredentials::default();
    for (name, value) in request.headers() {
        // A header value that is not valid text cannot be an API key; skip it
        // rather than reject the whole request.
        if let Some(provider) = provider_from_header(name)
            && let Ok(api_key) = value.to_str()
        {
            credentials.insert(provider, SecretString::from(api_key));
        }
    }

    if !credentials.is_empty() {
        request.extensions_mut().insert(credentials);
    }

    next.run(request).await
}

/// Parses the provider named by a credential header, or `None` when `name` is
/// not a `X-Smista-Provider-<provider>-Api-Key` header.
///
/// The `<provider>` segment is a canonical provider name (`anthropic`,
/// `openai`, …); any other non-empty segment names an OpenAI-compatible
/// instance, since `:` cannot appear in a header name. A canonical name always
/// wins, so an instance must not reuse one.
fn provider_from_header(name: &HeaderName) -> Option<Provider> {
    // `HeaderName` renders lowercase, matching the credential markers.
    let segment = name
        .as_str()
        .strip_prefix(PROVIDER_CREDENTIAL_PREFIX)?
        .strip_suffix(PROVIDER_CREDENTIAL_SUFFIX)?;
    if segment.is_empty() {
        return None;
    }

    match Provider::from_str(segment) {
        Ok(provider) => Some(provider),
        // A bare segment is an OpenAI-compatible instance name when it is a
        // valid one; otherwise the header names no known provider.
        Err(_) => is_openai_compatible_instance_name(segment)
            .then(|| Provider::OpenAICompatible(segment.to_string())),
    }
}

/// Returns whether `name` is a valid OpenAI-compatible instance name: a
/// non-empty run of lowercase ASCII letters, digits, `-` and `_`.
///
/// Mirrors the rule `smista-core` enforces on configured instance names, so a
/// header can only name an instance the configuration could also declare.
fn is_openai_compatible_instance_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{HeaderName, Method, Request};
    use axum::routing::get as get_route;
    use axum::{Extension, Router};
    use secrecy::{ExposeSecret as _, SecretString};
    use smista_core::model::Provider;
    use tower::ServiceExt as _;

    use super::{RequestCredentials, extract_provider_credentials, provider_from_header};

    /// Test handler that reports the Anthropic credential the middleware
    /// extracted, or `none` when no credential reached the request.
    async fn echo_anthropic(credentials: Option<Extension<RequestCredentials>>) -> String {
        credentials
            .and_then(|Extension(credentials)| {
                credentials
                    .get(&Provider::Anthropic)
                    .map(|key| key.expose_secret().to_string())
            })
            .unwrap_or_else(|| "none".to_string())
    }

    /// Builds a tiny router that runs the credential-extraction middleware in
    /// front of [`echo_anthropic`].
    fn probe_router() -> Router {
        Router::new()
            .route("/probe", get_route(echo_anthropic))
            .layer(axum::middleware::from_fn(extract_provider_credentials))
    }

    /// Sends `GET /probe` with the supplied headers and returns the body text.
    async fn probe(headers: &[(&str, &str)]) -> String {
        let mut builder = Request::builder().method(Method::GET).uri("/probe");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder
            .body(Body::empty())
            .expect("failed to build request");
        let response = probe_router()
            .oneshot(request)
            .await
            .expect("router failed to handle the request");
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("failed to read the response body");
        String::from_utf8(bytes.to_vec()).expect("response body was not UTF-8")
    }

    #[test]
    fn should_parse_canonical_provider_credential_header() {
        let name = HeaderName::from_static("x-smista-provider-anthropic-api-key");
        assert_eq!(provider_from_header(&name), Some(Provider::Anthropic));
    }

    #[test]
    fn should_parse_openai_compatible_instance_header() {
        let name = HeaderName::from_static("x-smista-provider-my-vllm-api-key");
        assert_eq!(
            provider_from_header(&name),
            Some(Provider::OpenAICompatible("my-vllm".to_string()))
        );
    }

    #[test]
    fn should_ignore_headers_that_are_not_provider_credentials() {
        for name in [
            "content-type",
            "authorization",
            // The provider segment is missing entirely.
            "x-smista-provider--api-key",
            // The closing `-api-key` suffix is missing.
            "x-smista-provider-anthropic",
            // The opening prefix is missing.
            "anthropic-api-key",
            // An invalid instance name (`.` is not allowed) is not a provider.
            "x-smista-provider-bad.name-api-key",
        ] {
            assert_eq!(
                provider_from_header(&HeaderName::from_static(name)),
                None,
                "{name} should not parse as a provider credential header"
            );
        }
    }

    #[tokio::test]
    async fn should_extract_provider_credential_into_request_extension() {
        let body = probe(&[("x-smista-provider-anthropic-api-key", "sk-test-123")]).await;
        assert_eq!(body, "sk-test-123");
    }

    #[tokio::test]
    async fn should_leave_no_extension_when_no_credentials_are_sent() {
        let body = probe(&[("content-type", "application/json")]).await;
        assert_eq!(body, "none");
    }

    #[test]
    fn should_redact_credentials_from_debug_output() {
        let mut credentials = RequestCredentials::default();
        credentials.insert(Provider::Anthropic, SecretString::from("super-secret-key"));
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains("super-secret-key"));
    }
}

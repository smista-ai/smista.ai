//! Per-request credentials passed to a [`Provider`](crate::provider::Provider)
//! when resolving or listing the models it offers.
//!
//! Credentials no longer live inside a provider. A provider holds only
//! connection and configuration facts and is built once, so a single long-lived
//! instance can serve many callers; the credentials travel with each request as
//! an [`Authentication`] value the caller supplies. A provider maps the variant
//! it receives onto its wire credentials, and rejects a variant it cannot use
//! with [`MissingCredentials`](smista_core::error::ProviderErrorCategory::MissingCredentials).

use secrecy::SecretString;
use smista_core::error::ProviderErrorCategory;
use smista_core::model::Provider as ProviderId;

use crate::ProviderResult;

/// Credentials a caller supplies per request when resolving or listing models.
///
/// Credentials are not always an API key: a local Ollama daemon is keyless, a
/// daemon behind a gateway needs custom headers, and the remote providers or
/// Ollama Cloud need a bearer key. This enum captures those shapes so the same
/// [`Provider`](crate::provider::Provider) can be built once without secrets and
/// authenticated differently on each call.
///
/// The value carries [`SecretString`]s, so it never derives [`serde`]
/// serialization or `ts-rs` exports, and its [`Debug`] redacts every secret.
///
/// # Examples
///
/// ```
/// use secrecy::SecretString;
/// use smista_providers::auth::Authentication;
///
/// // A keyless local endpoint.
/// let none = Authentication::None;
/// assert!(none.api_key().is_none());
///
/// // A bearer key.
/// let keyed = Authentication::ApiKey(SecretString::from("sk-x"));
/// assert!(keyed.api_key().is_some());
/// ```
pub enum Authentication {
    /// No credentials: a local keyless endpoint (a local Ollama daemon, a
    /// keyless OpenAI-compatible runtime).
    None,
    /// A bearer API key: Anthropic, OpenAI, Ollama Cloud, or a keyed
    /// OpenAI-compatible endpoint.
    ApiKey(SecretString),
    /// Arbitrary authentication headers: a local Ollama daemon fronted by a
    /// gateway that requires custom header credentials. Each pair is the header
    /// name and its secret value.
    Headers(Vec<(String, SecretString)>),
}

impl Authentication {
    /// Returns the bearer API key, or [`None`] when no key was supplied.
    ///
    /// Only the [`ApiKey`](Authentication::ApiKey) variant carries a key;
    /// [`None`](Authentication::None) and [`Headers`](Authentication::Headers)
    /// both return [`None`].
    pub fn api_key(&self) -> Option<&SecretString> {
        match self {
            Self::ApiKey(api_key) => Some(api_key),
            Self::None | Self::Headers(_) => None,
        }
    }

    /// Returns the authentication headers, empty unless the
    /// [`Headers`](Authentication::Headers) variant was supplied.
    pub fn headers(&self) -> &[(String, SecretString)] {
        match self {
            Self::Headers(headers) => headers,
            Self::None | Self::ApiKey(_) => &[],
        }
    }

    /// Returns the bearer API key, or a [`MissingCredentials`] error when the
    /// supplied variant carries no key.
    ///
    /// For providers that always require an API key (Anthropic, OpenAI). `model`
    /// names the model being resolved so the error points at the right
    /// reference.
    ///
    /// # Errors
    ///
    /// Returns a [`ProviderError`](smista_core::error::ProviderError) with
    /// category [`MissingCredentials`] when the variant is not
    /// [`ApiKey`](Authentication::ApiKey).
    ///
    /// [`MissingCredentials`]: ProviderErrorCategory::MissingCredentials
    pub fn require_api_key(
        &self,
        provider: ProviderId,
        model: &str,
    ) -> ProviderResult<&SecretString> {
        self.api_key().ok_or_else(|| {
            crate::error::provider_error(
                ProviderErrorCategory::MissingCredentials,
                provider,
                Some(model.to_string()),
                "an API key is required to authenticate this provider",
            )
        })
    }
}

// Carries credentials, so `Debug` is implemented by hand to redact every secret
// rather than derived. A leak test in this module's `tests` guards the redaction
// (M-PUBLIC-DEBUG).
impl std::fmt::Debug for Authentication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("Authentication::None"),
            Self::ApiKey(_) => f
                .debug_tuple("Authentication::ApiKey")
                .field(&"[redacted]")
                .finish(),
            Self::Headers(headers) => f
                .debug_struct("Authentication::Headers")
                .field("headers", &format_args!("[{} header(s)]", headers.len()))
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use secrecy::ExposeSecret;

    use super::*;

    #[test]
    fn api_key_is_some_only_for_the_api_key_variant() {
        assert!(Authentication::None.api_key().is_none());
        assert!(
            Authentication::Headers(vec![("X-Token".to_string(), SecretString::from("shhh"))])
                .api_key()
                .is_none()
        );
        assert_eq!(
            Authentication::ApiKey(SecretString::from("sk-x"))
                .api_key()
                .map(ExposeSecret::expose_secret),
            Some("sk-x")
        );
    }

    #[test]
    fn headers_are_present_only_for_the_headers_variant() {
        assert!(Authentication::None.headers().is_empty());
        assert!(
            Authentication::ApiKey(SecretString::from("sk-x"))
                .headers()
                .is_empty()
        );

        let headers =
            Authentication::Headers(vec![("X-Token".to_string(), SecretString::from("shhh"))]);
        assert_eq!(headers.headers().len(), 1);
        assert_eq!(headers.headers()[0].0, "X-Token");
    }

    #[test]
    fn debug_redacts_the_api_key() {
        let secret = "sk-ant-api03-TOPSECRETVALUE";
        let rendered = format!("{:?}", Authentication::ApiKey(SecretString::from(secret)));

        assert!(rendered.contains("ApiKey"));
        assert!(rendered.contains("[redacted]"));
        assert!(!rendered.contains(secret), "the API key leaked into Debug");
    }

    #[test]
    fn debug_redacts_header_values() {
        let secret = "proxy-bearer-1234";
        let rendered = format!(
            "{:?}",
            Authentication::Headers(vec![(
                "X-Proxy-Token".to_string(),
                SecretString::from(secret),
            )])
        );

        assert!(rendered.contains("Headers"));
        assert!(rendered.contains("1 header(s)"));
        assert!(
            !rendered.contains(secret),
            "a header value leaked into Debug"
        );
    }

    #[test]
    fn debug_renders_the_none_variant() {
        assert_eq!(
            format!("{:?}", Authentication::None),
            "Authentication::None"
        );
    }

    #[test]
    fn require_api_key_returns_the_key_for_the_api_key_variant() {
        let authentication = Authentication::ApiKey(SecretString::from("sk-x"));
        let key = authentication
            .require_api_key(ProviderId::Anthropic, "claude-haiku-4-5")
            .expect("an API key was supplied");

        assert_eq!(key.expose_secret(), "sk-x");
    }

    #[test]
    fn require_api_key_errors_with_missing_credentials_when_absent() {
        let error = Authentication::None
            .require_api_key(ProviderId::OpenAI, "gpt-5.4")
            .expect_err("no key was supplied");

        assert_eq!(error.category, ProviderErrorCategory::MissingCredentials);
        assert_eq!(error.provider, ProviderId::OpenAI);
        assert_eq!(error.model.as_deref(), Some("gpt-5.4"));
    }
}

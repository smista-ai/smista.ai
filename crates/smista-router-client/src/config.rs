//! Connection settings for a [`Client`](crate::Client).
//!
//! [`RouterClientConfig`] carries the router's base URL and the connect and
//! request timeouts a concrete client applies. The base URL is the scheme and
//! host only (for example `http://localhost:7331`); a client appends `/api/v1`
//! to it for the versioned endpoints and reaches `/status` at the root.

use std::time::Duration;

use url::Url;

/// Default router base URL: the loopback address and port the router listens on.
pub const DEFAULT_URL: &str = "http://localhost:7331";
/// Default time to wait for the TCP connection to be established.
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Default time to wait for a full request/response round-trip.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Connection settings shared by every concrete [`Client`](crate::Client).
#[derive(Debug, Clone)]
pub struct RouterClientConfig {
    /// Router base URL: scheme and host only. A client appends `/api/v1` for the
    /// versioned endpoints and reaches `/status` at the root.
    pub base_url: Url,
    /// How long to wait for the connection to be established.
    pub connect_timeout: Duration,
    /// How long to wait for a request to complete once connected.
    pub request_timeout: Duration,
}

impl Default for RouterClientConfig {
    /// Targets `http://localhost:7331` with the default timeouts.
    fn default() -> Self {
        Self {
            base_url: Url::parse(DEFAULT_URL).expect("default URL is a valid URL"),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

impl RouterClientConfig {
    /// Creates a config for `base_url` with the default timeouts.
    #[must_use]
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            ..Default::default()
        }
    }

    /// Sets the connect timeout, returning the updated config for chaining.
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the request timeout, returning the updated config for chaining.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_localhost_with_default_timeouts() {
        let config = RouterClientConfig::default();
        assert_eq!(config.base_url.as_str(), "http://localhost:7331/");
        assert_eq!(config.connect_timeout, DEFAULT_CONNECT_TIMEOUT);
        assert_eq!(config.request_timeout, DEFAULT_REQUEST_TIMEOUT);
    }

    #[test]
    fn should_keep_default_timeouts_when_built_from_a_url() {
        let url = Url::parse("https://router.example.com").unwrap();
        let config = RouterClientConfig::new(url.clone());
        assert_eq!(config.base_url, url);
        assert_eq!(config.connect_timeout, DEFAULT_CONNECT_TIMEOUT);
        assert_eq!(config.request_timeout, DEFAULT_REQUEST_TIMEOUT);
    }

    #[test]
    fn should_override_timeouts_through_the_builders() {
        let config = RouterClientConfig::default()
            .with_connect_timeout(Duration::from_secs(1))
            .with_request_timeout(Duration::from_secs(5));
        assert_eq!(config.connect_timeout, Duration::from_secs(1));
        assert_eq!(config.request_timeout, Duration::from_secs(5));
    }
}

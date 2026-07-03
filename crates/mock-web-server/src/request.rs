use axum::http::{HeaderMap, Method};
use serde::de::DeserializeOwned;
use url::Url;

/// One request received by the mock router.
#[derive(Debug, Clone)]
pub struct Request {
    /// Full request URL.
    pub url: Url,
    /// HTTP method.
    pub method: Method,
    /// HTTP headers.
    pub headers: HeaderMap,
    /// Raw request body.
    pub body: Vec<u8>,
}

impl Request {
    /// Decodes the request body as JSON.
    pub fn body_json<T>(&self) -> Result<T, serde_json::Error>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body)
    }
}

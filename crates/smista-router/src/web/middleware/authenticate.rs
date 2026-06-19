//! Middleware to authenticate requests by validating the Bearer token in the `Authorization` header.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};
use secrecy::SecretString;
use smista_core::api::ApiErrorCode;

use crate::auth::Authenticator;
use crate::web::AuthenticatedUser;
use crate::web::error::WebError;

/// The request header carrying the credential.
const AUTHORIZATION_HEADER: &str = "Authorization";
/// The scheme prefix that precedes the session token in [`AUTHORIZATION_HEADER`].
const API_KEY_PREFIX: &str = "Bearer ";

/// Authenticates a request by validating the Bearer token in its
/// `Authorization` header, rejecting the request when the token is missing,
/// malformed or invalid.
///
/// On success the resolved user id is inserted into the request extensions
/// (readable downstream as an `Extension<AuthenticatedUser>`) and the request is forwarded to
/// `next`. This guards only the protected routes, so it is attached with
/// `route_layer` rather than wrapping the whole application.
///
/// The error response is a structured [`WebError`] whose status and machine
/// readable code are:
///
/// - `401 missing_credentials` when the `Authorization` header is absent.
/// - `401 invalid_token` when the header is not valid text or does not start
///   with the `Bearer ` scheme.
/// - `401 invalid_token` when the token is malformed, unknown or does not match
///   the stored hash; `401 token_expired` when it is past its expiry and
///   `401 token_revoked` when it has been signed out; and `500 internal_error`
///   when authentication fails for a server-side reason (the mapping from
///   [`Authenticator::authenticate`]'s error). The expired and revoked codes are
///   disclosed only to a caller presenting the genuine secret.
pub(crate) async fn authenticate(
    State(authenticator): State<Arc<Authenticator>>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(auth_header) = request.headers().get(AUTHORIZATION_HEADER) else {
        return WebError::from_code(
            ApiErrorCode::MissingCredentials,
            "Authorization header is missing.",
        )
        .into_response();
    };

    // check the header starts with "Bearer " and extract the token
    let token: SecretString = match auth_header.to_str().map(|s| s.strip_prefix(API_KEY_PREFIX)) {
        Ok(Some(s)) => s.into(),
        _ => {
            return WebError::from_code(
                ApiErrorCode::InvalidToken,
                "Authorization header must start with 'Bearer '.",
            )
            .into_response();
        }
    };

    // authenticate the token and reject the request if it's invalid
    let user_id = match authenticator.authenticate(&token).await {
        Ok(user) => user,
        Err(e) => {
            tracing::debug!("authentication failed: {e}");
            return WebError::from(e).into_response();
        }
    };

    tracing::debug!("authenticated user {user_id}");
    request
        .extensions_mut()
        .insert(AuthenticatedUser { user_id, token });
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use axum::extract::Extension;
    use axum::http::{HeaderValue, Method, Request, StatusCode};
    use axum::routing::get;
    use axum::{Json, Router, middleware};
    use secrecy::ExposeSecret as _;
    use uuid::Uuid;

    use super::authenticate;
    use crate::auth::Authenticator;
    use crate::config::RouterConfig;
    use crate::web::AuthenticatedUser;
    use crate::web::test_support::{send, test_database};

    /// Probe handler that echoes the user id the middleware stored in the
    /// request extensions, so a test can assert the identity is propagated.
    async fn probe(Extension(user): Extension<AuthenticatedUser>) -> Json<Uuid> {
        Json(user.user_id)
    }

    /// Builds a single-route router guarded by [`authenticate`], returning it
    /// together with a valid token and the id of the user that token
    /// authenticates.
    async fn guarded_router() -> (Router, String, Uuid) {
        let database = test_database().await;
        let token_ttl =
            std::time::Duration::from_secs(RouterConfig::default().auth.token_ttl_seconds);
        let authenticator = std::sync::Arc::new(Authenticator::new(database, token_ttl));
        let user = authenticator
            .bootstrap_user()
            .await
            .expect("failed to bootstrap the test user");
        let session = authenticator
            .sign_in(&user.api_key)
            .await
            .expect("failed to sign the test user in");
        let token = session.token.expose_secret().to_string();
        let router = Router::new()
            .route("/probe", get(probe))
            .route_layer(middleware::from_fn_with_state(authenticator, authenticate));
        (router, token, user.user_id)
    }

    /// Builds a `GET /probe` request, optionally carrying an `Authorization`
    /// header value.
    fn probe_request(authorization: Option<HeaderValue>) -> Request<axum::body::Body> {
        let mut builder = Request::builder().method(Method::GET).uri("/probe");
        if let Some(value) = authorization {
            builder = builder.header("Authorization", value);
        }
        builder
            .body(axum::body::Body::empty())
            .expect("failed to build request")
    }

    #[tokio::test]
    async fn should_reject_a_request_without_an_authorization_header() {
        let (router, _, _) = guarded_router().await;
        let (status, body) = send(router, probe_request(None)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_reject_an_authorization_header_with_the_wrong_scheme() {
        let (router, _, _) = guarded_router().await;
        let request = probe_request(Some(HeaderValue::from_static("Basic dXNlcjpwYXNz")));
        let (status, body) = send(router, request).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token");
    }

    #[tokio::test]
    async fn should_reject_an_authorization_header_that_is_not_valid_text() {
        let (router, _, _) = guarded_router().await;
        // A non-visible-ASCII byte makes `HeaderValue::to_str` fail, exercising
        // the header-decoding error branch.
        let value = HeaderValue::from_bytes(&[0xff]).expect("failed to build header value");
        let (status, body) = send(router, probe_request(Some(value))).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token");
    }

    #[tokio::test]
    async fn should_reject_a_bearer_token_that_does_not_authenticate() {
        let (router, _, _) = guarded_router().await;
        let request = probe_request(Some(HeaderValue::from_static("Bearer not-a-valid-token")));
        let (status, body) = send(router, request).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token");
    }

    #[tokio::test]
    async fn should_accept_a_valid_token_and_propagate_the_user_id() {
        let (router, token, user_id) = guarded_router().await;
        let value = HeaderValue::from_str(&format!("Bearer {token}"))
            .expect("failed to build header value");
        let (status, body) = send(router, probe_request(Some(value))).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body.as_str(), Some(user_id.to_string().as_str()));
    }
}

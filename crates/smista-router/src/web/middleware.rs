//! Cross-cutting HTTP middleware.
//!
//! Four layers wrap routes, one module each:
//!
//! - [`log_requests()`] emits one structured event per request, keeping the
//!   query string and every header out of the logs so secrets cannot leak.
//! - [`reject_query_credentials()`] refuses any request that smuggles a
//!   credential through a query parameter.
//! - [`extract_provider_credentials()`] lifts any
//!   `X-Smista-Provider-<provider>-Api-Key` header into a [`RequestCredentials`]
//!   map stored in the request extensions, scoped to that single request.
//! - [`authenticate()`] validates the `Authorization` Bearer token and stores
//!   the resolved user id in the request extensions. It guards only the
//!   protected routes, not the public health check or auth bootstrap/sign-in.

mod authenticate;
mod extract_provider_credentials;
mod log_requests;
mod reject_query_credentials;

pub(crate) use authenticate::authenticate;
#[expect(
    unused_imports,
    reason = "read by provider-backed handlers landing in follow-up issues"
)]
pub(crate) use extract_provider_credentials::RequestCredentials;
pub(crate) use extract_provider_credentials::extract_provider_credentials;
pub(crate) use log_requests::log_requests;
pub(crate) use reject_query_credentials::reject_query_credentials;

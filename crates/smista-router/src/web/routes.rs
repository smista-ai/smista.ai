//! HTTP route handlers, one module per endpoint.
//!
//! Each submodule holds the handler for a single route and its tests; this
//! module re-exports those handlers for [`build_router`](crate::web) to mount.
//! The handlers split into public endpoints (`status`, `bootstrap`, `sign_in`)
//! and endpoints gated by the [`authenticate`](crate::web) middleware.

mod bootstrap;
mod continue_run;
mod create_session;
mod delete_session;
mod execute;
mod get_session;
mod get_traces;
mod list_models;
mod list_providers;
mod list_sessions;
mod me;
mod preview;
mod session_usage;
mod sign_in;
mod sign_out;
mod status;
mod update_session;

use axum::Json;
use axum::http::StatusCode;

// Re-export the utoipa-generated path structs so that openapi.rs can reference
// them as `routes::__path_<fn>` in `paths(...)`.
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::bootstrap::__path_bootstrap;
pub(crate) use self::bootstrap::bootstrap;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::continue_run::__path_continue_run;
pub(crate) use self::continue_run::continue_run;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::create_session::__path_create_session;
pub(crate) use self::create_session::create_session;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::delete_session::__path_delete_session;
pub(crate) use self::delete_session::delete_session;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::execute::__path_execute;
pub(crate) use self::execute::execute;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::get_session::__path_get_session;
pub(crate) use self::get_session::get_session;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::get_traces::__path_get_traces;
pub(crate) use self::get_traces::get_traces;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::list_models::__path_list_models;
pub(crate) use self::list_models::list_models;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::list_providers::__path_list_providers;
pub(crate) use self::list_providers::list_providers;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::list_sessions::__path_list_sessions;
pub(crate) use self::list_sessions::list_sessions;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::me::__path_me;
pub(crate) use self::me::me;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::preview::__path_preview;
pub(crate) use self::preview::preview;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::session_usage::__path_session_usage;
pub(crate) use self::session_usage::session_usage;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::sign_in::__path_sign_in;
pub(crate) use self::sign_in::sign_in;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::sign_out::__path_sign_out;
pub(crate) use self::sign_out::sign_out;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::status::__path_status;
pub(crate) use self::status::status;
#[cfg(all(test, feature = "openapi"))]
pub(crate) use self::update_session::__path_update_session;
pub(crate) use self::update_session::update_session;
use crate::web::error::WebError;

/// Result type for API endpoints, returning a JSON body and a status code.
type ApiResult<T> = Result<(StatusCode, Json<T>), WebError>;

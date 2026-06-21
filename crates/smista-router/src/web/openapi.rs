//! OpenAPI document for the router HTTP API.
//!
//! Compiled only under the `openapi` feature. `ApiDoc` collects every
//! `#[utoipa::path]`-annotated handler and the shared component schemas; the
//! `SecurityAddon` registers the auth schemes the operations reference. The
//! `gen_openapi_schema` test serializes the document to `docs/api/openapi.json`,
//! the artifact `just gen_openapi` regenerates and `just check_openapi` guards.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::web::routes;

/// Registers the security schemes the operations reference by name.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "used as a modifier by the `#[derive(OpenApi)]` macro on ApiDoc; never constructed directly"
    )
)]
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .as_mut()
            .expect("the derived document always has components");
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some("Session token from POST /api/v1/auth/sign-in"))
                    .build(),
            ),
        );
        components.add_security_scheme(
            "apiKey",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                "X-Smista-Api-Key",
                "User API key, presented only at sign-in",
            ))),
        );
    }
}

/// The OpenAPI document for the smista-router HTTP API.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "consumed via ApiDoc::openapi() in gen_openapi and the serving endpoint; not directly instantiated"
    )
)]
#[derive(OpenApi)]
#[openapi(
    info(
        title = "smista-router HTTP API",
        description = "Local-first deterministic model-routing API. See docs/api/http-api.md.",
        license(name = "Elastic-2.0")
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "health", description = "Liveness"),
        (name = "auth", description = "Authentication and identity"),
        (name = "sessions", description = "Session lifecycle"),
        (name = "execution", description = "Task execution, streaming and preview"),
        (name = "traces", description = "Routing traces"),
        (name = "usage", description = "Usage accounting"),
        (name = "llm", description = "Providers and models")
    ),
    paths(
        routes::status,
        routes::bootstrap,
        routes::sign_in,
        routes::sign_out,
        routes::me,
        routes::create_session,
        routes::list_sessions,
        routes::get_session,
        routes::update_session,
        routes::delete_session,
        routes::list_providers,
        routes::list_models,
        routes::execute,
        routes::continue_run,
        routes::stream,
        routes::preview,
        routes::latest_trace,
        routes::get_trace,
        routes::session_usage
    )
)]
pub(crate) struct ApiDoc;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use utoipa::OpenApi as _;

    use super::ApiDoc;

    /// Serializes the OpenAPI document to `docs/api/openapi.json` at the workspace
    /// root. Run by `just gen_openapi`; the committed artifact is the source the
    /// drift gate (`just check_openapi`) compares against.
    #[test]
    fn gen_openapi_schema() {
        let json = ApiDoc::openapi()
            .to_pretty_json()
            .expect("failed to serialize the OpenAPI document");
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop(); // crates/smista-router → crates/
        path.pop(); // crates/ → workspace root
        path.push("docs");
        path.push("api");
        path.push("openapi.json");
        std::fs::write(&path, format!("{json}\n"))
            .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
    }
}

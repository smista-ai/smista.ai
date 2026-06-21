//! `GET /api/v1/llm/providers` — list available providers.
//!
//! Protected endpoint. The router registers a provider only when it is
//! available — configured with usable credentials, or a base URL for a local
//! provider — so every entry this returns is ready to route to. Each entry is a
//! [`ProviderDescriptor`](smista_core::model::ProviderDescriptor) carrying the
//! provider id, a display name and a `local` flag, wrapped in a
//! [`ListProvidersResponse`].

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use smista_core::api::ListProvidersResponse;

use crate::web::AppState;
use crate::web::routes::ApiResult;

/// Handles `GET /api/v1/llm/providers`.
///
/// Reads the live provider registry off the router and returns one descriptor
/// per available provider. The order is unspecified, as the registry is keyed
/// by provider id.
pub(crate) async fn list_providers(
    State(state): State<AppState>,
) -> ApiResult<ListProvidersResponse> {
    let providers = state.router.list_providers().into_values().collect();

    Ok((StatusCode::OK, Json(ListProvidersResponse { providers })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use uuid::Uuid;

    use crate::web::test_support::{authenticated_router, get, get_with_token, send};

    #[tokio::test]
    async fn should_list_the_available_providers() {
        let (router, token) = authenticated_router().await;

        let (status, body) = send(router, get_with_token("/api/v1/llm/providers", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let providers = body["providers"]
            .as_array()
            .expect("providers was not an array");
        assert_eq!(providers.len(), 2);

        // The mock router exposes a local Ollama provider and a remote OpenAI
        // one; each entry carries the right id, display name and locality.
        let ollama = providers
            .iter()
            .find(|provider| provider["id"] == "ollama")
            .expect("the ollama provider was not listed");
        assert_eq!(ollama["display_name"], "Mock Local");
        assert_eq!(ollama["local"], true);

        let openai = providers
            .iter()
            .find(|provider| provider["id"] == "openai")
            .expect("the openai provider was not listed");
        assert_eq!(openai["display_name"], "Mock Remote");
        assert_eq!(openai["local"], false);
    }

    #[tokio::test]
    async fn should_carry_only_id_display_name_and_local_per_entry() {
        let (router, token) = authenticated_router().await;

        let (status, body) = send(router, get_with_token("/api/v1/llm/providers", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let providers = body["providers"]
            .as_array()
            .expect("providers was not an array");
        for provider in providers {
            let object = provider
                .as_object()
                .expect("an entry was not a JSON object");
            let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["display_name", "id", "local"]);
        }
    }

    #[tokio::test]
    async fn should_reject_a_request_without_a_token() {
        let (router, _token) = authenticated_router().await;

        let (status, body) = send(router, get("/api/v1/llm/providers")).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_credentials");
    }

    #[tokio::test]
    async fn should_reject_an_invalid_token() {
        let (router, _token) = authenticated_router().await;

        // A well-formed but unknown token never matches a stored session.
        let bogus = Uuid::new_v4().to_string();
        let (status, body) = send(router, get_with_token("/api/v1/llm/providers", &bogus)).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_token");
    }
}

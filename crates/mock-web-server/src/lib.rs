//! Configurable mock `smista-router` HTTP server for tests.
//!
//! [`MockRouter`] starts a real local HTTP server that answers the documented
//! router API endpoints with canned responses. Tests can configure a status per
//! endpoint, override success bodies, and inspect every request the mock
//! received.

pub mod defaults;

mod endpoint;
mod request;
mod response;
mod server;

pub use self::endpoint::{Endpoint, EndpointStatus};
pub use self::request::Request;
pub use self::response::{ResponseTemplate, api_error, sse};
pub use self::server::{MockRouter, MockRouterBuilder};

#[cfg(test)]
mod tests {
    use smista_sdk::core::api::{
        ApiError, ApiErrorCode, CreateSessionResponse, GetSessionResponse, StatusResponse,
        TurnEvent, TurnResponse,
    };
    use tokio_util::sync::CancellationToken;
    use uuid::Uuid;

    use super::*;

    /// Issues a GET against the running mock and decodes the JSON body.
    async fn get_json<T>(router: &MockRouter, path: &str) -> (u16, T)
    where
        T: serde::de::DeserializeOwned,
    {
        let response = reqwest::Client::new()
            .get(format!("{}{path}", router.uri()))
            .send()
            .await
            .expect("the request reaches the mock");
        let status = response.status().as_u16();
        let body = response.json().await.expect("the body decodes");
        (status, body)
    }

    #[tokio::test]
    async fn should_serve_the_default_status_body() {
        let router = MockRouter::start().await;
        let (status, body) = get_json::<StatusResponse>(&router, "/status").await;
        assert_eq!(status, 200);
        assert_eq!(body, defaults::status());
    }

    #[tokio::test]
    async fn should_serve_creation_with_a_201_status() {
        let router = MockRouter::start().await;
        let response = reqwest::Client::new()
            .post(format!("{}/api/v1/sessions", router.uri()))
            .send()
            .await
            .expect("the request reaches the mock");
        assert_eq!(response.status().as_u16(), 201);
        let body: CreateSessionResponse = response.json().await.expect("the body decodes");
        assert_eq!(body, defaults::create_session());
    }

    #[tokio::test]
    async fn should_match_a_session_id_path_segment() {
        let router = MockRouter::start().await;
        let path = format!("/api/v1/sessions/{}", Uuid::nil());
        let (status, body) = get_json::<GetSessionResponse>(&router, &path).await;
        assert_eq!(status, 200);
        assert_eq!(body, defaults::get_session());
    }

    #[tokio::test]
    async fn should_apply_a_per_endpoint_response_override() {
        let router = MockRouter::builder()
            .respond(
                Endpoint::Status,
                api_error(ApiErrorCode::InternalError, "boom"),
            )
            .start()
            .await;
        let (status, body) = get_json::<ApiError>(&router, "/status").await;
        assert_eq!(status, 500);
        assert_eq!(body.error.code, "internal_error");
        assert_eq!(body.error.message, "boom");
    }

    #[tokio::test]
    async fn should_apply_endpoint_status_overrides() {
        let router = MockRouter::builder()
            .endpoint_status(Endpoint::Status, EndpointStatus::Unauthorized)
            .endpoint_status(Endpoint::GetSession, EndpointStatus::NotFound)
            .start()
            .await;
        let (status, body) = get_json::<ApiError>(&router, "/status").await;
        assert_eq!(status, 401);
        assert_eq!(body.error.code, "missing_credentials");

        let path = format!("/api/v1/sessions/{}", Uuid::nil());
        let (status, body) = get_json::<ApiError>(&router, &path).await;
        assert_eq!(status, 404);
        assert_eq!(body.error.code, "session_not_found");
    }

    #[tokio::test]
    async fn should_update_endpoint_status_while_running() {
        let router = MockRouter::builder()
            .endpoint_status(Endpoint::Status, EndpointStatus::ServerError)
            .start()
            .await;

        let (status, body) = get_json::<ApiError>(&router, "/status").await;
        assert_eq!(status, 500);
        assert_eq!(body.error.code, "internal_error");

        router
            .set_endpoint_status(Endpoint::Status, EndpointStatus::Ok)
            .await;

        let (status, body) = get_json::<StatusResponse>(&router, "/status").await;
        assert_eq!(status, 200);
        assert_eq!(body, defaults::status());
    }

    #[tokio::test]
    #[should_panic(expected = "Endpoint Status does not support NotFound")]
    async fn should_reject_not_found_for_endpoints_without_resource_not_found() {
        MockRouter::builder()
            .endpoint_status(Endpoint::Status, EndpointStatus::NotFound)
            .start()
            .await;
    }

    #[tokio::test]
    async fn should_serve_a_streamed_turn_as_event_stream() {
        let events = [
            TurnEvent::TextDelta {
                delta: "Hello".to_owned(),
            },
            TurnEvent::TurnEnd(Box::new(defaults::turn())),
        ];
        let router = MockRouter::builder()
            .respond(Endpoint::Execute, sse(&events))
            .start()
            .await;
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/v1/sessions/{}/execute",
                router.uri(),
                Uuid::nil()
            ))
            .header("accept", "text/event-stream")
            .send()
            .await
            .expect("the request reaches the mock");
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream"),
        );
        let body = response.text().await.expect("the body is readable");
        assert!(body.contains("data: {\"type\":\"text_delta\",\"delta\":\"Hello\"}\n\n"));
        assert!(body.contains("\"type\":\"turn_end\""));
    }

    #[tokio::test]
    async fn should_expose_a_base_url_and_record_requests() {
        let router = MockRouter::start().await;
        assert_eq!(router.base_url().path(), "/");

        reqwest::Client::new()
            .get(format!("{}/status", router.uri()))
            .send()
            .await
            .expect("the request reaches the mock");

        let received = router.received_requests().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].url.path(), "/status");
    }

    #[tokio::test]
    async fn should_decode_the_default_turn_for_execute() {
        let router = MockRouter::start().await;
        let response = reqwest::Client::new()
            .post(format!(
                "{}/api/v1/sessions/{}/execute",
                router.uri(),
                Uuid::nil()
            ))
            .send()
            .await
            .expect("the request reaches the mock");
        assert_eq!(response.status().as_u16(), 200);
        let body: TurnResponse = response.json().await.expect("the body decodes");
        assert_eq!(body, defaults::turn());
    }

    #[tokio::test]
    async fn should_stop_when_cancelled() {
        let cancellation = CancellationToken::new();
        let router = MockRouter::run(cancellation.clone()).await;
        let uri = router.uri();

        reqwest::Client::new()
            .get(format!("{uri}/status"))
            .send()
            .await
            .expect("the request reaches the mock");

        cancellation.cancel();
        router.wait_stopped().await;

        reqwest::Client::new()
            .get(format!("{uri}/status"))
            .send()
            .await
            .expect_err("the mock should stop after cancellation");
    }
}

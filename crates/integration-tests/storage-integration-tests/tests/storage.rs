//! Integration tests for the remote SurrealDB storage backends.
//!
//! These tests start a real SurrealDB server in a container, so they require a
//! running Docker daemon. The server boots with only a root user; each test
//! provisions a namespace-scoped user before connecting, mirroring how a
//! deployment pre-provisions credentials for the router.

use secrecy::SecretString;
use smista_storage::database::surreal::{
    RemoteOptions, SurrealBackend, SurrealDatabase, SurrealOptions,
};
use surrealdb::engine::any;
use surrealdb::opt::auth::Root;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

/// The namespace user the remote tests authenticate as.
const APP_USER: &str = "app";
/// The password for `APP_USER`.
const APP_PASSWORD: &str = "app";

#[tokio::test]
async fn remote_http_connects_and_initializes() {
    let (_container, address) = surreal_container().await;
    provision_namespace_user(&address).await;

    SurrealDatabase::new(remote_options(SurrealBackend::Http(RemoteOptions {
        url: format!("http://{address}"),
        username: Some(APP_USER.to_string()),
        password: Some(SecretString::from(APP_PASSWORD.to_string())),
    })))
    .await
    .expect("failed to connect to SurrealDB over HTTP");
}

#[tokio::test]
async fn remote_ws_connects_and_initializes() {
    let (_container, address) = surreal_container().await;
    provision_namespace_user(&address).await;

    SurrealDatabase::new(remote_options(SurrealBackend::WebSocket(RemoteOptions {
        url: format!("ws://{address}"),
        username: Some(APP_USER.to_string()),
        password: Some(SecretString::from(APP_PASSWORD.to_string())),
    })))
    .await
    .expect("failed to connect to SurrealDB over WebSocket");
}

fn remote_options(backend: SurrealBackend) -> SurrealOptions {
    SurrealOptions {
        namespace: "test".to_string(),
        db: "test".to_string(),
        backend,
    }
}

/// Starts a SurrealDB container and returns it with its `host:port` address.
///
/// The container is held by the caller; dropping it stops the container, so
/// the test must keep it alive for the duration of the connection.
async fn surreal_container() -> (ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("surrealdb/surrealdb", "latest")
        .with_exposed_port(8000.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Started web server"))
        .with_cmd([
            "start",
            "--user",
            "root",
            "--pass",
            "root",
            "--bind",
            "0.0.0.0:8000",
            "memory",
        ])
        .start()
        .await
        .expect("failed to start SurrealDB container");

    let host = container.get_host().await.expect("failed to resolve host");
    let port = container
        .get_host_port_ipv4(8000.tcp())
        .await
        .expect("failed to resolve mapped port");

    (container, format!("{host}:{port}"))
}

/// Provisions the namespace user that the remote tests authenticate as.
///
/// The server starts with only a root user, so it must define the test
/// namespace and a namespace-scoped user before `SurrealDatabase::new` signs in
/// with namespace credentials.
async fn provision_namespace_user(address: &str) {
    let root = any::connect(format!("ws://{address}"))
        .await
        .expect("failed to connect as root");
    root.signin(Root {
        username: "root".to_string(),
        password: "root".to_string(),
    })
    .await
    .expect("failed to sign in as root");

    root.query("DEFINE NAMESPACE IF NOT EXISTS test")
        .await
        .expect("failed to define namespace")
        .check()
        .expect("namespace definition failed");
    root.use_ns("test").await.expect("failed to use namespace");
    root.query(format!(
        "DEFINE DATABASE IF NOT EXISTS test; \
         DEFINE USER IF NOT EXISTS {APP_USER} ON NAMESPACE PASSWORD '{APP_PASSWORD}' ROLES OWNER"
    ))
    .await
    .expect("failed to define database and user")
    .check()
    .expect("database/user definition failed");
}

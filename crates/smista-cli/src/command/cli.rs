//! Main command to run (which runs without a subcommand).
//!
//! It start the TUI and the smista-router client; it is basically the main user interface for smista.ai

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use smista_sdk::client::{ApiKey, Client as _, ReqwestClient};
use tokio_util::sync::CancellationToken;
use url::{Host, Url};

use crate::app::AppContext;
use crate::args::RouterArgs;
use crate::config::Config;
use crate::credentials::{
    ApiKeyStorage, CredentialsStorage, E2eeKeysCredentials, ProvidersCredentials,
};
use crate::skills::SkillStore;

/// Timeout for waiting for the router to be up and running, when auto-starting it.
const WAIT_FOR_ROUTER_TIMEOUT: Duration = Duration::from_secs(30);

/// Runs smista.ai TUI and smista-router client; base subcommand.
///
/// The `initial_prompt` is an optional string that will be used as the initial prompt for the TUI.
/// If it is `None`, the TUI will start with an empty prompt.
pub async fn run(
    _initial_prompt: Option<String>,
    enforce_keyring: bool,
    log_file: Option<&Path>,
    log_filter: &str,
) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    tracing::info!(
        "running smista-cli main command; cwd is {cwd}",
        cwd = cwd.display()
    );
    let credentials = Arc::new(CredentialsStorage::new(enforce_keyring)?);
    tracing::debug!(
        "credentials storage initialized with backend {backend}",
        backend = credentials.backend()
    );

    // load config
    let config =
        crate::config::load_and_validate(&cwd).context("Failed to load CLI configuration")?;

    // setup router client
    let router_client =
        crate::client::config_client(&config).context("Failed to configure router client")?;

    // auto start router
    check_and_auto_start_router(
        &config,
        &router_client,
        router_client.base_url(),
        log_file,
        log_filter,
    )
    .await
    .context("Failed to auto-start router")?;

    let api_key = Arc::new(ApiKeyStorage::new(credentials.clone(), &cwd));
    let router_client = sign_in_router_client(router_client, api_key.get()?)
        .await
        .context("Failed to sign in to router")?;

    // build context
    tracing::info!("app initialization completed; building app context");
    let _context = AppContext {
        api_key,
        config: Arc::new(config),
        e2ee_keys: Arc::new(E2eeKeysCredentials::new(credentials.clone(), &cwd)),
        exit: CancellationToken::new(),
        providers_credentials: Arc::new(ProvidersCredentials::new(credentials, &cwd)),
        router_client: Arc::new(router_client),
        skills_store: Arc::new(SkillStore::discover(&cwd)),
        cwd,
    };

    // TODO: link initial_prompt to the TUI; the tui will log it into the chat, and push to the router client.
    // TODO: link App::new().run().await here.

    tracing::info!("smista-cli main command finished");
    Ok(())
}

/// Checks if the router is running, and if not, tries to auto-start it.
///
/// The logic is as follows:
///
/// 1. check the router status, by querying the `/status` endpoint; if it is running, return Ok.
/// 2. If it is NOT ok, check if the `router_url` host is local or not (localhost, 127.0.0.1, ::1, etc);
/// 3. If it is NOT local, return an error, because we cannot auto-start a remote router.
/// 4. If it IS local, check if the config has `auto_start` enabled.
/// 5. If it is NOT enabled, return an error, because we cannot auto-start the router if the user did not enable it.
/// 6. If it IS enabled, try to auto-start the router by running the `smista-router`
/// 7. wait for the router to be up and running, by querying the `/status` endpoint in a loop, with a timeout of `WAIT_FOR_ROUTER_TIMEOUT`.
async fn check_and_auto_start_router(
    config: &Config,
    client: &ReqwestClient,
    router_url: &Url,
    log_file: Option<&Path>,
    log_filter: &str,
) -> anyhow::Result<()> {
    check_and_auto_start_router_with(
        config,
        client,
        router_url,
        log_file,
        log_filter,
        &CommandRouterStarter,
    )
    .await
}

/// Checks router availability and starts a local router when policy permits it.
///
/// The first `/status` check decides whether any startup work is needed. A
/// reachable router is accepted regardless of host locality or `auto_start`.
/// When `/status` fails, only loopback hosts with `auto_start = true` are
/// started through `starter`; every other case returns an actionable error.
async fn check_and_auto_start_router_with<S>(
    config: &Config,
    client: &ReqwestClient,
    router_url: &Url,
    log_file: Option<&Path>,
    log_filter: &str,
    starter: &S,
) -> anyhow::Result<()>
where
    S: RouterStarter,
{
    tracing::info!("checking if router is running at {router_url}");
    let Err(err) = client.status().await else {
        tracing::debug!("router is running at {router_url}; no need to auto-start");
        return Ok(());
    };
    tracing::debug!(
        "router is not running at {router_url}: {err}; checking if it can be auto-started"
    );

    let is_local = router_url.host().map(is_loopback).unwrap_or_default();
    if !is_local {
        anyhow::bail!("router at {router_url} is unreachable and is not local; cannot auto-start");
    }

    tracing::debug!(
        "router is not running at {router_url}, but it is local; checking config for auto-start"
    );
    if !config.router.auto_start {
        anyhow::bail!(
            "router is not running at {router_url}, and auto-start is disabled in config. Run `smista start` to start the router."
        );
    }

    // start router
    tracing::debug!(
        "auto-starting router at {router_url} with log file {log_file:?} and log filter {log_filter}"
    );
    starter
        .start(
            RouterArgs {
                config: None,
                pidfile: None,
                foreground: false,
                otel: false,
                no_otel: false,
                otel_endpoint: None,
                otel_protocol: None,
                otel_sample_ratio: None,
                otel_service_name: None,
            },
            log_file,
            log_filter,
        )
        .await?;
    tracing::info!("router auto-started at {router_url}; waiting for it to be up and running");

    wait_for_router(client)
        .await
        .context("Failed to wait for router to be up and running")?;

    Ok(())
}

/// Starts a router for the auto-start path.
trait RouterStarter {
    /// Starts the router with the same arguments used by `smista start`.
    fn start<'a>(
        &'a self,
        args: RouterArgs,
        log_file: Option<&'a Path>,
        log_filter: &'a str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

/// Production [`RouterStarter`] that delegates to the router command.
struct CommandRouterStarter;

impl RouterStarter for CommandRouterStarter {
    fn start<'a>(
        &'a self,
        args: RouterArgs,
        log_file: Option<&'a Path>,
        log_filter: &'a str,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(crate::command::router::start(args, log_file, log_filter))
    }
}

/// Returns whether `host` names the local loopback interface.
fn is_loopback(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    }
}

/// Waits until the router reports a successful `/status` response.
///
/// # Errors
///
/// Returns an error if the router does not become reachable within
/// [`WAIT_FOR_ROUTER_TIMEOUT`].
async fn wait_for_router(client: &ReqwestClient) -> anyhow::Result<()> {
    tracing::debug!("waiting for router to be up and running");

    let start = Instant::now();
    loop {
        match client.status().await {
            Ok(_) => {
                tracing::info!("started router is up and running");
                break;
            }
            Err(err) => {
                if start.elapsed() > WAIT_FOR_ROUTER_TIMEOUT {
                    anyhow::bail!(
                        "router did not start within {timeout} seconds: {err}",
                        timeout = WAIT_FOR_ROUTER_TIMEOUT.as_secs()
                    );
                }
                tracing::debug!(
                    "router is not up yet (elapsed: {elapsed}): {err}; retrying in 1s",
                    elapsed = start.elapsed().as_secs()
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    Ok(())
}

/// Signs in `client` using a configured router API key.
///
/// The returned client is the same handle, updated with the held session token,
/// so storing it in [`AppContext`] preserves authenticated state.
///
/// # Errors
///
/// Returns an error when no API key is configured or the router rejects sign-in.
async fn sign_in_router_client(
    client: ReqwestClient,
    api_key: Option<ApiKey>,
) -> anyhow::Result<ReqwestClient> {
    let Some(api_key) = api_key else {
        anyhow::bail!(
            "no router API key configured. Run `smista login` or `smista apikey set <api-key>` first."
        );
    };
    let user_id = api_key.user_id()?;
    tracing::debug!("signing in router client for user {user_id}");
    let client = client.with_api_key(api_key);
    let response = client.sign_in().await.context("router sign-in failed")?;
    tracing::info!(
        "router sign-in succeeded for user {user_id}; session token expires at: {expires_at}",
        user_id = user_id,
        expires_at = response.expires_at
    );

    Ok(client)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::path::Path;
    use std::str::FromStr as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use smista_mock_web_server::{Endpoint, EndpointStatus, MockRouter, defaults};
    use smista_sdk::client::{ApiKey, Client as _, RouterClientConfig};

    use super::*;

    #[test]
    fn should_detect_loopback_hosts() {
        assert!(is_loopback(Host::Domain("localhost")));
        assert!(is_loopback(Host::Domain("LOCALHOST")));
        assert!(is_loopback(Host::Ipv4(Ipv4Addr::new(127, 0, 0, 1))));
        assert!(is_loopback(Host::Ipv4(Ipv4Addr::new(127, 1, 2, 3))));
        assert!(is_loopback(Host::Ipv6(Ipv6Addr::LOCALHOST)));

        assert!(!is_loopback(Host::Domain("router.example.com")));
        assert!(!is_loopback(Host::Ipv4(Ipv4Addr::new(192, 168, 1, 2))));
        assert!(!is_loopback(Host::Ipv6(Ipv6Addr::UNSPECIFIED)));
    }

    #[tokio::test]
    async fn should_accept_running_local_router_without_auto_start() {
        let router = MockRouter::start().await;
        let config = config_with_auto_start(false);
        let client = client_for(&router.base_url());
        let starter = TestRouterStarter::default();

        check_and_auto_start_router_with(
            &config,
            &client,
            &router.base_url(),
            None,
            "off",
            &starter,
        )
        .await
        .expect("running local routers are accepted");

        assert_eq!(starter.calls(), 0);
    }

    #[tokio::test]
    async fn should_accept_running_local_router_with_auto_start() {
        let router = MockRouter::start().await;
        let config = config_with_auto_start(true);
        let client = client_for(&router.base_url());
        let starter = TestRouterStarter::default();

        check_and_auto_start_router_with(
            &config,
            &client,
            &router.base_url(),
            None,
            "off",
            &starter,
        )
        .await
        .expect("running local routers are accepted");

        assert_eq!(starter.calls(), 0);
    }

    #[tokio::test]
    async fn should_accept_running_remote_router_without_auto_start() {
        let router = MockRouter::start().await;
        let config = config_with_auto_start(false);
        let client = client_for(&router.base_url());
        let router_url = Url::parse("https://router.example.com").unwrap();
        let starter = TestRouterStarter::default();

        check_and_auto_start_router_with(&config, &client, &router_url, None, "off", &starter)
            .await
            .expect("running remote routers are accepted");

        assert_eq!(starter.calls(), 0);
    }

    #[tokio::test]
    async fn should_accept_running_remote_router_with_auto_start() {
        let router = MockRouter::start().await;
        let config = config_with_auto_start(true);
        let client = client_for(&router.base_url());
        let router_url = Url::parse("https://router.example.com").unwrap();
        let starter = TestRouterStarter::default();

        check_and_auto_start_router_with(&config, &client, &router_url, None, "off", &starter)
            .await
            .expect("running remote routers are accepted");

        assert_eq!(starter.calls(), 0);
    }

    #[tokio::test]
    async fn should_reject_stopped_local_router_when_auto_start_is_disabled() {
        let router = failed_status_router().await;
        let config = config_with_auto_start(false);
        let client = client_for(&router.base_url());
        let starter = TestRouterStarter::default();

        let error = check_and_auto_start_router_with(
            &config,
            &client,
            &router.base_url(),
            None,
            "off",
            &starter,
        )
        .await
        .expect_err("disabled auto-start rejects a stopped local router");

        assert_eq!(starter.calls(), 0);
        assert!(
            error.to_string().contains("smista start"),
            "error should tell the user to run smista start: {error}"
        );
    }

    #[tokio::test]
    async fn should_reject_stopped_remote_router_as_unreachable() {
        let router = failed_status_router().await;
        let config = config_with_auto_start(true);
        let client = client_for(&router.base_url());
        let router_url = Url::parse("https://router.example.com").unwrap();
        let starter = TestRouterStarter::default();

        let error =
            check_and_auto_start_router_with(&config, &client, &router_url, None, "off", &starter)
                .await
                .expect_err("stopped remote routers are unreachable");

        assert_eq!(starter.calls(), 0);
        assert!(
            error.to_string().contains("unreachable"),
            "error should explain that the router is unreachable: {error}"
        );
    }

    #[tokio::test]
    async fn should_start_stopped_local_router_when_auto_start_is_enabled() {
        let router = failed_status_router().await;
        let config = config_with_auto_start(true);
        let client = client_for(&router.base_url());
        let starter = TestRouterStarter::unblocks_status(router);

        check_and_auto_start_router_with(
            &config,
            &client,
            &starter.router_url(),
            None,
            "off",
            &starter,
        )
        .await
        .expect("enabled auto-start starts a stopped local router");

        assert_eq!(starter.calls(), 1);
    }

    #[tokio::test]
    async fn should_sign_in_router_client_with_stored_api_key() {
        let router = MockRouter::start().await;
        let client = client_for(&router.base_url());
        let api_key = ApiKey::from_str(&defaults::bootstrap().api_key).unwrap();

        let client = sign_in_router_client(client, Some(api_key))
            .await
            .expect("sign-in succeeds");

        client
            .me()
            .await
            .expect("the signed-in client authenticates");
    }

    #[tokio::test]
    async fn should_reject_missing_router_api_key_before_context_creation() {
        let router = MockRouter::start().await;
        let client = client_for(&router.base_url());

        let error = sign_in_router_client(client, None)
            .await
            .expect_err("a missing API key prevents app context creation");

        assert!(
            error.to_string().contains("smista apikey set"),
            "error should tell the user how to configure an API key: {error}"
        );
    }

    fn config_with_auto_start(auto_start: bool) -> Config {
        let mut config = Config::default();
        config.router.auto_start = auto_start;
        config
    }

    fn client_for(url: &Url) -> ReqwestClient {
        ReqwestClient::new(
            RouterClientConfig::new(url.clone())
                .with_connect_timeout(Duration::from_millis(50))
                .with_request_timeout(Duration::from_millis(200)),
        )
        .expect("the test client builds")
    }

    async fn failed_status_router() -> MockRouter {
        MockRouter::builder()
            .endpoint_status(Endpoint::Status, EndpointStatus::ServerError)
            .start()
            .await
    }

    #[derive(Default)]
    struct TestRouterStarter {
        calls: Arc<AtomicUsize>,
        router: Option<MockRouter>,
    }

    impl TestRouterStarter {
        fn unblocks_status(router: MockRouter) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                router: Some(router),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn router_url(&self) -> Url {
            self.router
                .as_ref()
                .expect("the starter holds a mock router")
                .base_url()
        }
    }

    impl RouterStarter for TestRouterStarter {
        fn start<'a>(
            &'a self,
            _args: RouterArgs,
            _log_file: Option<&'a Path>,
            _log_filter: &'a str,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                if let Some(router) = &self.router {
                    router
                        .set_endpoint_status(Endpoint::Status, EndpointStatus::Ok)
                        .await;
                }
                Ok(())
            })
        }
    }
}

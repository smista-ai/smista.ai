//! `status` command implementation.
//!
//! Gets the status of the smista.ai router and print status.

use anyhow::Context as _;
use smista_sdk::client::Client as _;

use crate::args::StatusArgs;

pub async fn run(StatusArgs { url }: StatusArgs) -> anyhow::Result<()> {
    // resolve url; if specified, use it; otherwise, load the CLI config
    let mut config = crate::config::load_and_validate(&std::env::current_dir()?)
        .context("Failed to load CLI configuration")?;
    if let Some(url) = url {
        config.router.url = Some(url.to_string());
    }

    let client =
        crate::client::config_client(&config).context("Failed to configure router client")?;

    let status = client
        .status()
        .await
        .context("Failed to get router status")?;

    println!(
        r#"smista.ai router ("{url}") status: "{status}" - version: "{version}""#,
        url = client.base_url(),
        status = status.status,
        version = status.version
    );

    if status.status != "ok" {
        anyhow::bail!("Router is not healthy. Please check the router logs for more information.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use smista_mock_web_server::MockRouter;

    use super::*;

    #[tokio::test]
    async fn should_query_the_configured_router_status_url() {
        let router = MockRouter::start().await;

        run(StatusArgs {
            url: Some(router.base_url()),
        })
        .await
        .expect("status command succeeds against the mock router");

        let received = router.received_requests().await;
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].url.path(), "/status");
    }
}

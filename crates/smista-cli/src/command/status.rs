//! `status` command implementation.
//!
//! Gets the status of the smista.ai router and print status.

use anyhow::Context as _;
use smista_sdk::client::{Client as _, ROUTER_DEFAULT_URL, ReqwestClient, RouterClientConfig};
use url::Url;

use crate::args::StatusArgs;

pub async fn run(StatusArgs { url }: StatusArgs) -> anyhow::Result<()> {
    // resolve url; if specified, use it; otherwise, load the CLI config
    let url = match url {
        Some(url) => url,
        None => {
            let config = crate::config::load_and_validate(&std::env::current_dir()?)
                .context("Failed to load CLI configuration")?;

            let url = config
                .router
                .url
                .unwrap_or_else(|| ROUTER_DEFAULT_URL.to_string());

            Url::parse(&url).context("Failed to parse router URL")?
        }
    };

    let config = RouterClientConfig::new(url.clone());
    let reqwest_client = ReqwestClient::new(config)?;

    let status = reqwest_client
        .status()
        .await
        .context("Failed to get router status")?;

    println!(
        r#"smista.ai router ("{url}") status: "{status}" - version: "{version}""#,
        url = url,
        status = status.status,
        version = status.version
    );

    if status.status != "ok" {
        anyhow::bail!("Router is not healthy. Please check the router logs for more information.");
    }

    Ok(())
}

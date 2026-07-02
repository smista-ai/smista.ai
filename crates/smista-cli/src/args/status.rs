//! `smista status` command arguments.

use url::Url;

/// Arguments for `smista status`.
///
/// `smista status` queries the router's `/status` endpoint and prints the result.
#[derive(Debug, clap::Args)]
pub struct StatusArgs {
    /// URL of the router to query. Defaults to configured router URL.
    #[clap(long = "url")]
    pub url: Option<Url>,
}

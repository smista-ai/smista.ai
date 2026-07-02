//! `smista apikey` arguments.

/// Arguments for `smista apikey`.
///
/// The command stores, checks or removes the smista.ai router API key in either
/// the project-local credential scope or the global scope selected by
/// `--global`.
#[derive(Debug, clap::Args)]
pub struct ApikeyArgs {
    /// Credential operation to perform.
    #[clap(subcommand)]
    pub command: ApikeyCommand,
    /// Whether to use the global credential scope instead of the local one.
    #[clap(short = 'g', long = "global")]
    pub global: bool,
}

/// The action a `smista apikey` invocation performs.
#[derive(Debug, clap::Subcommand)]
pub enum ApikeyCommand {
    /// Check whether the API key is set.
    #[clap(name = "check", alias = "get")]
    Check,
    /// Remove the API key.
    #[clap(name = "remove", alias = "delete", alias = "rm")]
    Remove,
    /// Set or replace the API key.
    #[clap(name = "set", alias = "add")]
    Set {
        /// API key to store.
        api_key: String,
    },
}

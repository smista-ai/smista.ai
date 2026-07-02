//! `smista credentials` arguments.

use smista_sdk::core::model::Provider;

/// Arguments for `smista credentials`.
///
/// The command stores, checks or removes one provider credential in either the
/// project-local credential scope or the global scope selected by `--global`.
#[derive(Debug, clap::Args)]
pub struct CredentialsArgs {
    /// Credential operation to perform.
    #[clap(subcommand)]
    pub command: CredentialsCommand,
    /// Whether to use the global credential scope instead of the local one.
    #[clap(short = 'g', long = "global")]
    pub global: bool,
}

/// The action a `smista credentials` invocation performs.
#[derive(Debug, clap::Subcommand)]
pub enum CredentialsCommand {
    /// Check whether credentials for a provider are present.
    #[clap(name = "check", alias = "get")]
    Check {
        /// Provider to check credentials for.
        provider: Provider,
    },
    /// Remove credentials for a provider.
    #[clap(name = "remove", alias = "delete", alias = "rm")]
    Remove {
        /// Provider to remove credentials for.
        provider: Provider,
    },
    /// Set or replace credentials for a provider.
    #[clap(name = "set", alias = "add")]
    Set {
        /// Provider to set credentials for.
        provider: Provider,
        /// API key to store for the provider.
        api_key: String,
    },
}

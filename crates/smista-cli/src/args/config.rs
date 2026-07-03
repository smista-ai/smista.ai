//! `smista config` arguments.

use std::path::PathBuf;

/// Arguments for `smista config`.
///
/// The command creates or manages the configuration files used by smista.ai:
///
/// - router runtime configuration
/// - global CLI configuration
/// - project CLI configuration
#[derive(Debug, clap::Args)]
pub struct ConfigArgs {
    /// Configuration operation to perform.
    #[clap(subcommand)]
    pub command: ConfigCommand,
    /// Path to the configuration file to operate on.
    ///
    /// When omitted, the command uses the default path for the selected
    /// configuration scope.
    #[clap(short = 'c', long = "config")]
    pub path: Option<PathBuf>,
}

/// The action a `smista config` invocation performs.
#[derive(Debug, clap::Subcommand)]
pub enum ConfigCommand {
    /// Validate the contents of a configuration file.
    Check {
        /// Configuration file to check.
        #[clap(value_enum, default_value_t = ConfigScope::Project)]
        scope: ConfigScope,
    },
    /// Edit a configuration file in the default editor.
    Edit {
        /// Configuration file to edit.
        #[clap(value_enum, default_value_t = ConfigScope::Project)]
        scope: ConfigScope,
    },
    /// Create a starter configuration file.
    Init {
        /// Replace the target configuration file if it already exists.
        #[clap(short = 'f', long = "force")]
        force: bool,
        /// Configuration file to initialize.
        #[clap(value_enum, default_value_t = ConfigScope::Project)]
        scope: ConfigScope,
    },
    /// Show the path to a configuration file.
    Path {
        /// Configuration file to show the path for.
        #[clap(value_enum)]
        scope: Option<ConfigScope>,
    },
    /// Display the contents of a configuration file.
    Show {
        /// Configuration file to show.
        #[clap(value_enum)]
        scope: Option<ConfigScope>,
    },
}

/// The configuration file selected by `smista config` commands.
#[derive(Debug, Clone, Copy, Eq, PartialEq, clap::ValueEnum)]
pub enum ConfigScope {
    /// Router runtime configuration.
    #[clap(name = "router")]
    Router,
    /// Global CLI configuration.
    #[clap(name = "global")]
    Global,
    /// Project CLI configuration.
    #[clap(name = "project")]
    Project,
}

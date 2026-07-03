mod apikey;
mod config;
mod credentials;
mod router;
mod status;

use std::path::PathBuf;

pub use self::apikey::{ApikeyArgs, ApikeyCommand};
pub use self::config::{ConfigArgs, ConfigCommand, ConfigScope};
pub use self::credentials::{CredentialsArgs, CredentialsCommand};
pub use self::router::{RouterArgs, StopArgs};
pub use self::status::StatusArgs;

/// Top-level `smista` command-line arguments.
///
/// A subcommand selects the action to run; the logging flags are global, so
/// they may appear before or after the subcommand.
#[derive(Debug, clap::Parser)]
#[command(version)]
pub struct Args {
    /// The subcommand to run.
    #[clap(subcommand)]
    pub command: Option<Command>,
    /// Prompt to run when no subcommand is selected.
    #[clap(value_name = "PROMPT")]
    pub prompt: Option<String>,
    /// Require the operating-system keyring for credential storage.
    ///
    /// By default, the CLI falls back to file-backed storage when the keyring is
    /// unavailable.
    #[clap(long = "enforce-keyring")]
    pub enforce_keyring: bool,
    /// Set log file path. If not set, logs will be printed to stdout. Can also be set via the `SMISTA_ROUTER_LOG_FILE` environment variable.
    #[clap(
        short = 'L',
        global = true,
        long = "log-file",
        env = "SMISTA_ROUTER_LOG_FILE"
    )]
    pub log_file: Option<PathBuf>,
    /// Log level filter. Can also be set via the `SMISTA_ROUTER_LOG_FILTER` environment variable.
    #[clap(
        short = 'l',
        global = true,
        long = "log-filter",
        env = "SMISTA_ROUTER_LOG_FILTER",
        default_value = "off"
    )]
    pub log_filter: String,
}

impl Args {
    /// Whether this invocation runs the router in the current process.
    ///
    /// The foreground router owns its telemetry setup, so logging is left for it
    /// to initialize rather than being set up up front.
    #[must_use]
    pub fn is_foreground_start(&self) -> bool {
        matches!(&self.command, Some(Command::Start(args)) if args.foreground)
    }
}

/// The action a `smista` invocation performs.
#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Manage smista.ai API key for the CLI.
    Apikey(ApikeyArgs),
    /// Manage configuration files for smista.ai.
    Config(ConfigArgs),
    /// Manage credentials for interacting with the LLMs.
    Credentials(CredentialsArgs),
    /// Start a local router. Daemonizes by default; pass `--foreground` to run
    /// it in the current process.
    Start(RouterArgs),
    /// Get router status. Queries the router `/status` endpoint and prints the result.
    Status(StatusArgs),
    /// Stop the local router recorded in the pidfile.
    Stop(StopArgs),
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser as _;
    use smista_router::config::{OpenTelemetryConfig, OtlpProtocol};

    use super::*;

    fn start_args(argv: &[&str]) -> RouterArgs {
        let Some(Command::Start(start)) = Args::parse_from(argv).command else {
            panic!("expected the start command");
        };
        start
    }

    #[test]
    fn should_default_opentelemetry_flags_off() {
        let start = start_args(&["smista", "start"]);
        assert!(!start.otel);
        assert!(!start.no_otel);
        let resolved = start.resolve_opentelemetry(OpenTelemetryConfig::default());
        assert!(!resolved.enabled);
    }

    #[test]
    fn should_let_command_line_override_the_file_for_opentelemetry() {
        let start = start_args(&[
            "smista",
            "start",
            "--otel",
            "--otel-endpoint",
            "http://collector:4317",
            "--otel-protocol",
            "http-binary",
            "--otel-service-name",
            "router-a",
            "--otel-sample-ratio",
            "0.25",
        ]);
        // The file disables export; the command line must win.
        let resolved = start.resolve_opentelemetry(OpenTelemetryConfig::default());
        assert!(resolved.enabled);
        assert_eq!(resolved.endpoint, "http://collector:4317");
        assert_eq!(resolved.protocol, OtlpProtocol::HttpBinary);
        assert_eq!(resolved.service_name, "router-a");
        assert!((resolved.sample_ratio - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn should_disable_opentelemetry_from_the_command_line() {
        let start = start_args(&["smista", "start", "--no-otel"]);
        let from_file = OpenTelemetryConfig {
            enabled: true,
            ..OpenTelemetryConfig::default()
        };
        let resolved = start.resolve_opentelemetry(from_file);
        assert!(!resolved.enabled);
    }

    #[test]
    fn should_keep_the_file_value_when_no_toggle_is_given() {
        let start = start_args(&["smista", "start", "--otel-sample-ratio", "0.5"]);
        let from_file = OpenTelemetryConfig {
            enabled: true,
            ..OpenTelemetryConfig::default()
        };
        let resolved = start.resolve_opentelemetry(from_file);
        assert!(resolved.enabled);
        assert!((resolved.sample_ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn should_reject_enabling_and_disabling_opentelemetry_together() {
        assert!(Args::try_parse_from(["smista", "start", "--otel", "--no-otel"]).is_err());
    }

    #[test]
    fn should_apply_defaults_for_start() {
        let args = Args::parse_from(["smista", "start"]);

        assert!(args.log_file.is_none());
        assert_eq!(args.log_filter, "off");
        assert!(!args.enforce_keyring);
        let Some(Command::Start(start)) = args.command else {
            panic!("expected the start command");
        };
        assert!(start.config.is_none());
        assert!(start.pidfile.is_none());
        assert!(!start.foreground);
    }

    #[test]
    fn should_parse_global_flags_before_the_subcommand() {
        let args = Args::parse_from([
            "smista",
            "--log-file",
            "/var/log/smista.log",
            "--log-filter",
            "debug",
            "start",
        ]);

        assert_eq!(
            args.log_file.as_deref(),
            Some(Path::new("/var/log/smista.log"))
        );
        assert_eq!(args.log_filter, "debug");
        assert!(matches!(args.command, Some(Command::Start(_))));
    }

    #[test]
    fn should_parse_start_flags() {
        let args = Args::parse_from([
            "smista",
            "start",
            "--config",
            "/etc/smista/router.toml",
            "--pidfile",
            "/run/user/1000/smista/router.pid",
            "--foreground",
        ]);

        let Some(Command::Start(start)) = args.command else {
            panic!("expected the start command");
        };
        assert_eq!(
            start.config.as_deref(),
            Some(Path::new("/etc/smista/router.toml"))
        );
        assert_eq!(
            start.pidfile.as_deref(),
            Some(Path::new("/run/user/1000/smista/router.pid"))
        );
        assert!(start.foreground);
    }

    #[test]
    fn should_parse_stop_flags() {
        let args = Args::parse_from([
            "smista",
            "stop",
            "--pidfile",
            "/run/user/1000/smista/router.pid",
        ]);

        let Some(Command::Stop(stop)) = args.command else {
            panic!("expected the stop command");
        };
        assert_eq!(
            stop.pidfile.as_deref(),
            Some(Path::new("/run/user/1000/smista/router.pid"))
        );
    }

    #[test]
    fn should_parse_main_command_flags_without_a_subcommand() {
        let args = Args::parse_from(["smista", "--enforce-keyring"]);

        assert!(args.command.is_none());
        assert!(args.enforce_keyring);
    }

    #[test]
    fn should_parse_prompt_without_a_subcommand() {
        let args = Args::parse_from(["smista", "write a changelog"]);

        assert!(args.command.is_none());
        assert_eq!(args.prompt.as_deref(), Some("write a changelog"));
    }

    #[test]
    fn should_reject_prompt_after_a_subcommand() {
        assert!(Args::try_parse_from(["smista", "status", "write a changelog"]).is_err());
    }

    #[test]
    fn should_parse_credentials_add_command() {
        let args = Args::parse_from(["smista", "credentials", "add", "openai", "sk-test"]);

        let Some(Command::Credentials(credentials)) = args.command else {
            panic!("expected the credentials command");
        };
        assert!(!credentials.global);
        assert!(matches!(
            credentials.command,
            CredentialsCommand::Set {
                provider: smista_sdk::core::model::Provider::OpenAI,
                api_key
            } if api_key == "sk-test"
        ));
    }

    #[test]
    fn should_parse_config_init_without_scope_as_project() {
        let args = Args::parse_from(["smista", "config", "init"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(config.path.is_none());
        assert!(matches!(
            config.command,
            ConfigCommand::Init {
                scope: ConfigScope::Project,
                force: false
            }
        ));
    }

    #[test]
    fn should_parse_config_init_project_scope() {
        let args = Args::parse_from(["smista", "config", "init", "project"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(matches!(
            config.command,
            ConfigCommand::Init {
                scope: ConfigScope::Project,
                force: false
            }
        ));
    }

    #[test]
    fn should_parse_config_init_force_and_path() {
        let args = Args::parse_from([
            "smista",
            "config",
            "--config",
            "/tmp/smista.toml",
            "init",
            "--force",
            "router",
        ]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert_eq!(config.path.as_deref(), Some(Path::new("/tmp/smista.toml")));
        assert!(matches!(
            config.command,
            ConfigCommand::Init {
                scope: ConfigScope::Router,
                force: true
            }
        ));
    }

    #[test]
    fn should_parse_config_show_without_scope_as_effective_config() {
        let args = Args::parse_from(["smista", "config", "show"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(config.path.is_none());
        assert!(matches!(
            config.command,
            ConfigCommand::Show { scope: None }
        ));
    }

    #[test]
    fn should_parse_config_show_with_scope_as_single_layer() {
        let args = Args::parse_from(["smista", "config", "show", "global"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(matches!(
            config.command,
            ConfigCommand::Show {
                scope: Some(ConfigScope::Global)
            }
        ));
    }

    #[test]
    fn should_parse_config_path_without_scope_as_all_paths() {
        let args = Args::parse_from(["smista", "config", "path"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(config.path.is_none());
        assert!(matches!(
            config.command,
            ConfigCommand::Path { scope: None }
        ));
    }

    #[test]
    fn should_parse_config_path_with_scope_as_single_path() {
        let args = Args::parse_from(["smista", "config", "path", "router"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(matches!(
            config.command,
            ConfigCommand::Path {
                scope: Some(ConfigScope::Router)
            }
        ));
    }

    #[test]
    fn should_parse_config_check_without_scope_as_project() {
        let args = Args::parse_from(["smista", "config", "check"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(matches!(
            config.command,
            ConfigCommand::Check {
                scope: ConfigScope::Project
            }
        ));
    }

    #[test]
    fn should_parse_config_edit_with_scope() {
        let args = Args::parse_from(["smista", "config", "edit", "global"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert!(matches!(
            config.command,
            ConfigCommand::Edit {
                scope: ConfigScope::Global
            }
        ));
    }

    #[test]
    fn should_parse_config_path_with_explicit_config_path() {
        let args = Args::parse_from(["smista", "config", "--config", "/tmp/custom.toml", "path"]);

        let Some(Command::Config(config)) = args.command else {
            panic!("expected the config command");
        };
        assert_eq!(config.path.as_deref(), Some(Path::new("/tmp/custom.toml")));
        assert!(matches!(
            config.command,
            ConfigCommand::Path { scope: None }
        ));
    }

    #[test]
    fn should_parse_credentials_global_flag() {
        let args = Args::parse_from([
            "smista",
            "credentials",
            "--global",
            "add",
            "anthropic",
            "sk-ant-test",
        ]);

        let Some(Command::Credentials(credentials)) = args.command else {
            panic!("expected the credentials command");
        };
        assert!(credentials.global);
        assert!(matches!(
            credentials.command,
            CredentialsCommand::Set {
                provider: smista_sdk::core::model::Provider::Anthropic,
                api_key
            } if api_key == "sk-ant-test"
        ));
    }

    #[test]
    fn should_parse_credentials_command_aliases() {
        let set = Args::parse_from(["smista", "credentials", "set", "gemini", "gm-test"]);
        let get = Args::parse_from(["smista", "credentials", "get", "gemini"]);
        let delete = Args::parse_from(["smista", "credentials", "delete", "gemini"]);
        let rm = Args::parse_from(["smista", "credentials", "rm", "gemini"]);

        assert!(matches!(
            set.command,
            Some(Command::Credentials(CredentialsArgs {
                command: CredentialsCommand::Set {
                    provider: smista_sdk::core::model::Provider::Gemini,
                    ..
                },
                ..
            }))
        ));
        assert!(matches!(
            get.command,
            Some(Command::Credentials(CredentialsArgs {
                command: CredentialsCommand::Check {
                    provider: smista_sdk::core::model::Provider::Gemini
                },
                ..
            }))
        ));
        assert!(matches!(
            delete.command,
            Some(Command::Credentials(CredentialsArgs {
                command: CredentialsCommand::Remove {
                    provider: smista_sdk::core::model::Provider::Gemini
                },
                ..
            }))
        ));
        assert!(matches!(
            rm.command,
            Some(Command::Credentials(CredentialsArgs {
                command: CredentialsCommand::Remove {
                    provider: smista_sdk::core::model::Provider::Gemini
                },
                ..
            }))
        ));
    }

    #[test]
    fn should_parse_openai_compatible_credentials_provider() {
        let args = Args::parse_from(["smista", "credentials", "check", "openai-compat:my-vllm"]);

        let Some(Command::Credentials(credentials)) = args.command else {
            panic!("expected the credentials command");
        };
        assert!(matches!(
            credentials.command,
            CredentialsCommand::Check {
                provider: smista_sdk::core::model::Provider::OpenAICompatible(name)
            } if name == "my-vllm"
        ));
    }

    #[test]
    fn should_reject_unknown_credentials_provider() {
        assert!(Args::try_parse_from(["smista", "credentials", "check", "cohere"]).is_err());
    }

    #[test]
    fn should_parse_apikey_set_command() {
        let args = Args::parse_from([
            "smista",
            "apikey",
            "set",
            "sk-smista-api01-00000000000000000000000000000000-secret",
        ]);

        let Some(Command::Apikey(apikey)) = args.command else {
            panic!("expected the apikey command");
        };
        assert!(!apikey.global);
        assert!(matches!(
            apikey.command,
            ApikeyCommand::Set { api_key }
                if api_key == "sk-smista-api01-00000000000000000000000000000000-secret"
        ));
    }

    #[test]
    fn should_parse_apikey_global_flag() {
        let args = Args::parse_from([
            "smista",
            "apikey",
            "--global",
            "add",
            "sk-smista-api01-00000000000000000000000000000000-secret",
        ]);

        let Some(Command::Apikey(apikey)) = args.command else {
            panic!("expected the apikey command");
        };
        assert!(apikey.global);
        assert!(matches!(apikey.command, ApikeyCommand::Set { .. }));
    }

    #[test]
    fn should_parse_apikey_command_aliases() {
        let set = Args::parse_from([
            "smista",
            "apikey",
            "add",
            "sk-smista-api01-00000000000000000000000000000000-secret",
        ]);
        let get = Args::parse_from(["smista", "apikey", "get"]);
        let delete = Args::parse_from(["smista", "apikey", "delete"]);
        let rm = Args::parse_from(["smista", "apikey", "rm"]);

        assert!(matches!(
            set.command,
            Some(Command::Apikey(ApikeyArgs {
                command: ApikeyCommand::Set { .. },
                ..
            }))
        ));
        assert!(matches!(
            get.command,
            Some(Command::Apikey(ApikeyArgs {
                command: ApikeyCommand::Check,
                ..
            }))
        ));
        assert!(matches!(
            delete.command,
            Some(Command::Apikey(ApikeyArgs {
                command: ApikeyCommand::Remove,
                ..
            }))
        ));
        assert!(matches!(
            rm.command,
            Some(Command::Apikey(ApikeyArgs {
                command: ApikeyCommand::Remove,
                ..
            }))
        ));
    }
}

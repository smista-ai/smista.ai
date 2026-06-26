use std::path::PathBuf;

use smista_router::config::OtlpProtocol;

/// Arguments for `smista start`.
///
/// `smista start` launches a local router. By default it daemonizes — it spawns
/// the router as a detached background process and returns — so the shell is
/// free again. Pass `--foreground` to run the router in the current process
/// instead, which is what a service manager wants.
///
/// The `--otel*` flags configure OpenTelemetry trace export. Each overrides the
/// matching setting in the router configuration file's `[opentelemetry]`
/// section, so an operator can turn export on and point it at a collector
/// without editing the file.
#[derive(Debug, clap::Args)]
pub struct RouterArgs {
    /// Path to the router configuration file. When omitted, the router loads the
    /// per-user `router.toml`. Can also be set via the `SMISTA_ROUTER_CONFIG`
    /// environment variable.
    #[clap(short = 'c', long = "config", env = "SMISTA_ROUTER_CONFIG")]
    pub config: Option<PathBuf>,
    /// Path to the pidfile the router records its process id in. When omitted, a
    /// per-user default under the runtime directory is used. Can also be set via
    /// the `SMISTA_ROUTER_PIDFILE` environment variable.
    #[clap(short = 'p', long = "pidfile", env = "SMISTA_ROUTER_PIDFILE")]
    pub pidfile: Option<PathBuf>,
    /// Run the router in the foreground instead of daemonizing it.
    #[clap(short = 'f', long = "foreground")]
    pub foreground: bool,
    /// Enable OpenTelemetry trace export, overriding the configuration file.
    #[clap(long = "otel", conflicts_with = "no_otel")]
    pub otel: bool,
    /// Disable OpenTelemetry trace export, overriding the configuration file.
    #[clap(long = "no-otel")]
    pub no_otel: bool,
    /// OTLP collector endpoint to export traces to, e.g. `http://localhost:4317`.
    #[clap(long = "otel-endpoint")]
    pub otel_endpoint: Option<String>,
    /// OTLP wire protocol: `grpc` or `http-binary`.
    #[clap(long = "otel-protocol")]
    pub otel_protocol: Option<OtlpProtocol>,
    /// Service name reported on exported traces.
    #[clap(long = "otel-service-name")]
    pub otel_service_name: Option<String>,
    /// Fraction of traces to sample, from `0.0` (none) to `1.0` (all).
    #[clap(long = "otel-sample-ratio")]
    pub otel_sample_ratio: Option<f64>,
}

impl RouterArgs {
    /// Applies the `--otel*` overrides on top of the configuration file's
    /// settings, returning the effective OpenTelemetry configuration.
    ///
    /// A setting given on the command line always wins over the file. `--otel`
    /// and `--no-otel` toggle [`enabled`](smista_router::config::OpenTelemetryConfig::enabled);
    /// when neither is given the file's value is kept.
    #[must_use]
    pub fn resolve_opentelemetry(
        &self,
        mut config: smista_router::config::OpenTelemetryConfig,
    ) -> smista_router::config::OpenTelemetryConfig {
        if self.no_otel {
            config.enabled = false;
        } else if self.otel {
            config.enabled = true;
        }
        if let Some(endpoint) = &self.otel_endpoint {
            config.endpoint = endpoint.clone();
        }
        if let Some(protocol) = self.otel_protocol {
            config.protocol = protocol;
        }
        if let Some(service_name) = &self.otel_service_name {
            config.service_name = service_name.clone();
        }
        if let Some(sample_ratio) = self.otel_sample_ratio {
            config.sample_ratio = sample_ratio;
        }
        config
    }
}

/// Arguments for `smista stop`.
///
/// `smista stop` reads the router's process id from the pidfile and asks that
/// process to shut down.
#[derive(Debug, clap::Args)]
pub struct StopArgs {
    /// Path to the pidfile written by `smista start`. When omitted, the same
    /// per-user default `smista start` uses is read. Can also be set via the
    /// `SMISTA_ROUTER_PIDFILE` environment variable.
    #[clap(short = 'p', long = "pidfile", env = "SMISTA_ROUTER_PIDFILE")]
    pub pidfile: Option<PathBuf>,
}

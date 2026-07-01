//! OpenTelemetry trace export for the in-process router.
//!
//! The CLI owns observability for the locally started router: it builds an OTLP
//! exporter from the resolved [`OpenTelemetryConfig`] and hands back a
//! `tracing` [`Layer`] that the logging setup composes alongside the formatting
//! layer. Export is layered on top of the existing logging and never changes it;
//! when disabled, no exporter or layer is built and nothing is sent.
//!
//! Only span and trace metadata is exported. Secrets are never recorded on
//! spans (see the project's logging rules), so they are never sent to the
//! collector.

use anyhow::Context as _;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use smista_router::config::{OpenTelemetryConfig, OtlpProtocol};
use tracing::Subscriber;
use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

/// Instrumentation scope name reported for the router's spans.
const TRACER_NAME: &str = "smista-router";

/// Builds the OpenTelemetry `tracing` layer and its tracer provider.
///
/// The returned [`TelemetryGuard`] owns the provider; keep it alive for the
/// lifetime of the process so spans keep exporting, and drop it on shutdown to
/// flush any buffered traces.
///
/// # Errors
///
/// Returns an error if the OTLP exporter cannot be constructed for the
/// configured endpoint and protocol.
pub fn layer<S>(config: &OpenTelemetryConfig) -> anyhow::Result<(impl Layer<S>, TelemetryGuard)>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    tracing::debug!(
        otel.endpoint = %config.endpoint,
        otel.service_name = %config.service_name,
        otel.sample_ratio = config.sample_ratio,
        "configuring OpenTelemetry trace export"
    );

    let exporter = build_exporter(config)?;

    // ParentBased keeps whole traces consistent: a child honours the parent's
    // sampling decision, and only roots are sampled at the configured ratio.
    let sampler = Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(config.sample_ratio)));
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_resource(
            Resource::builder()
                .with_service_name(config.service_name.clone())
                .build(),
        )
        .build();

    let tracer = provider.tracer(TRACER_NAME);
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Ok((layer, TelemetryGuard::new(provider)))
}

/// Builds the OTLP span exporter for the configured wire protocol.
fn build_exporter(config: &OpenTelemetryConfig) -> anyhow::Result<SpanExporter> {
    let exporter = match config.protocol {
        OtlpProtocol::Grpc => SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&config.endpoint)
            .build()
            .context("failed to build the OTLP/gRPC span exporter")?,
        OtlpProtocol::HttpBinary => SpanExporter::builder()
            .with_http()
            .with_endpoint(&config.endpoint)
            .with_protocol(Protocol::HttpBinary)
            .build()
            .context("failed to build the OTLP/HTTP span exporter")?,
    };
    Ok(exporter)
}

/// Owns the OpenTelemetry tracer provider for the process lifetime.
///
/// Dropping the guard shuts the provider down, flushing any traces still
/// buffered by the batch exporter.
#[derive(Debug)]
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl TelemetryGuard {
    /// Wraps a live provider.
    fn new(provider: SdkTracerProvider) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    /// A guard for the disabled case, owning no provider and doing nothing on
    /// drop.
    pub fn disabled() -> Self {
        Self { provider: None }
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        let Some(provider) = self.provider.take() else {
            return;
        };
        if let Err(err) = provider.shutdown() {
            tracing::warn!(
                error.message = %err,
                "failed to flush OpenTelemetry traces on shutdown"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::registry::Registry;

    use super::*;

    fn config(protocol: OtlpProtocol, endpoint: &str) -> OpenTelemetryConfig {
        OpenTelemetryConfig {
            enabled: true,
            endpoint: endpoint.to_string(),
            protocol,
            service_name: "test-service".to_string(),
            sample_ratio: 0.5,
        }
    }

    #[tokio::test]
    async fn should_build_a_grpc_export_layer() {
        let config = config(OtlpProtocol::Grpc, "http://localhost:4317");
        let (_layer, guard) = match layer::<Registry>(&config) {
            Ok(built) => built,
            Err(err) => panic!("the gRPC layer should build: {err}"),
        };
        // Dropping the guard shuts the provider down without a live collector.
        drop(guard);
    }

    #[tokio::test]
    async fn should_build_an_http_export_layer() {
        let config = config(OtlpProtocol::HttpBinary, "http://localhost:4318");
        let (_layer, guard) = match layer::<Registry>(&config) {
            Ok(built) => built,
            Err(err) => panic!("the HTTP layer should build: {err}"),
        };
        drop(guard);
    }

    #[test]
    fn should_do_nothing_when_dropping_a_disabled_guard() {
        let guard = TelemetryGuard::disabled();
        drop(guard);
    }
}

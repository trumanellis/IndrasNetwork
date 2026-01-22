//! OpenTelemetry integration for distributed tracing
//!
//! This module provides OpenTelemetry tracer setup and integration
//! with the tracing ecosystem.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::{
    trace::{RandomIdGenerator, Sampler, TracerProvider},
    Resource,
};
use opentelemetry_otlp::WithExportConfig;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::registry::LookupSpan;

use crate::config::OtelConfig;

/// Error type for OpenTelemetry setup
#[derive(Debug, thiserror::Error)]
pub enum OtelError {
    #[error("Failed to create OTLP exporter: {0}")]
    ExporterError(String),
    #[error("Failed to create tracer provider: {0}")]
    ProviderError(String),
    #[error("OpenTelemetry trace error: {0}")]
    TraceError(#[from] opentelemetry::trace::TraceError),
}

/// Initialize OpenTelemetry and return a tracing layer
///
/// This sets up an OTLP exporter that sends traces to a collector
/// (e.g., Jaeger, Zipkin, or an OpenTelemetry Collector).
///
/// The returned layer is generic over any subscriber that implements LookupSpan.
pub fn init_otel_layer<S>(
    config: &OtelConfig,
) -> Result<OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>, OtelError>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    // Build resource attributes
    let mut resource_attrs = vec![opentelemetry::KeyValue::new(
        "service.name",
        config.service_name.clone(),
    )];

    for (key, value) in &config.resource_attributes {
        resource_attrs.push(opentelemetry::KeyValue::new(key.clone(), value.clone()));
    }

    let resource = Resource::new(resource_attrs);

    // Create OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .build()
        .map_err(|e| OtelError::ExporterError(e.to_string()))?;

    // Create sampler based on sample ratio
    let sampler = if config.sample_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sample_ratio <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_ratio)
    };

    // Build tracer provider
    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .build();

    // Get a tracer
    let tracer = provider.tracer("indras-network");

    // Create the OpenTelemetry layer
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // Store the provider globally so it doesn't get dropped
    // This is important - if the provider is dropped, traces won't be exported
    opentelemetry::global::set_tracer_provider(provider);

    Ok(layer)
}

/// Shutdown OpenTelemetry, flushing any pending traces
///
/// Call this before your application exits to ensure all traces are exported.
pub fn shutdown_otel() {
    opentelemetry::global::shutdown_tracer_provider();
}

/// Create an OpenTelemetry layer with default configuration from environment
///
/// Uses OTEL_EXPORTER_OTLP_ENDPOINT and OTEL_SERVICE_NAME environment variables.
pub fn init_otel_from_env<S>(
) -> Result<OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>, OtelError>
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    let config = OtelConfig::from_env();
    init_otel_layer(&config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otel_config_defaults() {
        let config = OtelConfig::default();
        assert!(!config.endpoint.is_empty());
        assert!(!config.service_name.is_empty());
    }

    // Note: Full OTel integration tests require a running collector
    // and are better suited for integration tests
}

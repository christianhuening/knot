//! OpenTelemetry OTLP exporter setup.
//!
//! API notes for opentelemetry-otlp 0.27.0 / opentelemetry_sdk 0.27.1:
//!
//! - `SpanExporter::builder().with_tonic().with_endpoint(…).build()` — unchanged.
//! - The SDK struct is `TracerProvider` (NOT `SdkTracerProvider`; that name is unreleased).
//! - `TracerProviderBuilder::with_batch_exporter` no longer takes a runtime argument.
//! - `Resource::new(kvs)` is the current constructor (builder_empty is unreleased).
//! - `global::shutdown_tracer_provider()` is gone; call `provider.shutdown()` directly.
//! - The caller must hold the returned `TracerProvider` and call `shutdown()` on drop/exit.

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    #[error("otlp: {0}")]
    Otlp(String),
    #[error("subscriber: {0}")]
    Subscriber(String),
}

/// Initialise the global tracing subscriber WITH an OTLP layer.
///
/// Call this **instead of** `logging::init` when OTLP is enabled.
/// Returns the `TracerProvider`; the caller must call `.shutdown()` at
/// process exit to flush any in-flight spans.
pub fn init_with_otlp(
    level: &str,
    format: &str,
    endpoint: &str,
    service_name: &str,
) -> Result<opentelemetry_sdk::trace::TracerProvider, TracingError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| TracingError::Otlp(e.to_string()))?;

    let resource = Resource::new([KeyValue::new("service.name", service_name.to_string())]);

    // In 0.27.1, with_batch_exporter still takes a runtime argument.
    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(service_name.to_string());
    opentelemetry::global::set_tracer_provider(provider.clone());

    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(otel_layer);

    match format {
        "json" => registry
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|e| TracingError::Subscriber(e.to_string())),
        _ => registry
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .map_err(|e| TracingError::Subscriber(e.to_string())),
    }?;

    Ok(provider)
}

/// Flush and shut down the OTLP exporter.
///
/// Pass the `TracerProvider` returned by `init_with_otlp`.
/// Any error is logged but not propagated (best-effort at shutdown time).
pub fn shutdown(provider: opentelemetry_sdk::trace::TracerProvider) {
    if let Err(e) = provider.shutdown() {
        eprintln!("knot-obs: OTLP shutdown error: {e}");
    }
}

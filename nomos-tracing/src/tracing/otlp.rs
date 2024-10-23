// std
use std::error::Error;
// crates
use opentelemetry::{global, trace::TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{BatchConfig, Sampler, Tracer};
use serde::{Deserialize, Serialize};
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::registry::LookupSpan;
use url::Url;
// internal

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OtlpTracingConfig {
    pub endpoint: Url,
}

pub fn create_otlp_tracing_layer<S>(
    config: OtlpTracingConfig,
) -> Result<OpenTelemetryLayer<S, Tracer>, Box<dyn Error>>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let otel_exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(config.endpoint);
    let tracer_provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_trace_config(opentelemetry_sdk::trace::Config::default().with_sampler(
            Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(1.0))),
        ))
        .with_batch_config(BatchConfig::default())
        .with_exporter(otel_exporter)
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    global::set_tracer_provider(tracer_provider.clone());
    let tracer: opentelemetry_sdk::trace::Tracer = tracer_provider.tracer("BasicTracer");

    Ok(OpenTelemetryLayer::new(tracer))
}

// std
use opentelemetry_otlp::{ExportConfig, WithExportConfig};
use opentelemetry_sdk::{runtime, Resource};
use std::error::Error;
// crates
use opentelemetry::KeyValue;
use serde::{Deserialize, Serialize};
use tracing::Subscriber;
use tracing_opentelemetry::MetricsLayer;
use tracing_subscriber::registry::LookupSpan;
use url::Url;
// internal

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OtlpMetricsConfig {
    pub endpoint: Url,
    pub service_name: String,
}

pub fn create_otlp_tracing_layer<S>(
    config: OtlpMetricsConfig,
) -> Result<MetricsLayer<S>, Box<dyn Error + Send + Sync>>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let resource = Resource::new(vec![KeyValue::new(
        opentelemetry_semantic_conventions::resource::SERVICE_NAME,
        config.service_name,
    )]);

    let export_config = ExportConfig {
        endpoint: config.endpoint.into(),
        ..ExportConfig::default()
    };
    let provider = opentelemetry_otlp::new_pipeline()
        .metrics(runtime::Tokio)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(export_config),
        )
        .with_resource(resource)
        .build()?;

    Ok(MetricsLayer::new(provider))
}

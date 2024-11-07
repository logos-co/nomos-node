use nomos_tracing::{
    logging::loki::LokiConfig, metrics::otlp::OtlpMetricsConfig, tracing::otlp::OtlpTracingConfig,
};
use nomos_tracing_service::{
    FilterLayer, LoggerLayer, MetricsLayer, TracingLayer, TracingSettings,
};
use tracing::Level;

#[derive(Clone, Default)]
pub struct GeneralTracingConfig {
    pub tracing_settings: TracingSettings,
}

impl GeneralTracingConfig {
    #[allow(dead_code)]
    fn local_debug_tracing(id: usize) -> Self {
        let host_identifier = format!("node-{id}");
        Self {
            tracing_settings: TracingSettings {
                logger: LoggerLayer::Loki(LokiConfig {
                    endpoint: "http://localhost:3100".try_into().unwrap(),
                    host_identifier: host_identifier.clone(),
                }),
                tracing: TracingLayer::Otlp(OtlpTracingConfig {
                    endpoint: "http://localhost:4317".try_into().unwrap(),
                    sample_ratio: 0.1,
                }),
                filter: FilterLayer::EnvFilter(nomos_tracing::filter::envfilter::EnvFilterConfig {
                    // Allow events only from modules that matches the regex, if it matches - use
                    // provided tracing level. Libp2p and risc0 related crates are very log
                    // intensive in debug mode.
                    filters: [("nomos", "debug")]
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                }),
                metrics: MetricsLayer::Otlp(OtlpMetricsConfig {
                    endpoint: "http://127.0.0.1:9090/api/v1/otlp/v1/metrics"
                        .try_into()
                        .unwrap(),
                    host_identifier,
                }),
                level: Level::DEBUG,
            },
        }
    }
}

pub fn create_tracing_configs(ids: &[[u8; 32]]) -> Vec<GeneralTracingConfig> {
    #[cfg(feature = "debug")]
    {
        create_debug_configs(ids)
    }

    #[cfg(not(feature = "debug"))]
    {
        create_default_configs(ids)
    }
}

#[allow(dead_code)]
fn create_debug_configs(ids: &[[u8; 32]]) -> Vec<GeneralTracingConfig> {
    ids.iter()
        .enumerate()
        .map(|(i, _)| GeneralTracingConfig::local_debug_tracing(i))
        .collect()
}

#[allow(dead_code)]
fn create_default_configs(ids: &[[u8; 32]]) -> Vec<GeneralTracingConfig> {
    ids.iter()
        .map(|_| GeneralTracingConfig::default())
        .collect()
}

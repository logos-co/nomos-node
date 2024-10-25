use nomos_tracing::{logging::loki::LokiConfig, tracing::otlp::OtlpTracingConfig};
use nomos_tracing_service::{FilterLayer, LoggerLayer, TracingLayer, TracingSettings};
use tracing::Level;

#[derive(Clone, Default)]
pub struct GeneralTracingConfig {
    pub tracing_settings: TracingSettings,
}

impl GeneralTracingConfig {
    #[allow(dead_code)]
    fn local_debug_tracing(id: usize) -> Self {
        Self {
            tracing_settings: TracingSettings {
                logger: LoggerLayer::Loki(LokiConfig {
                    endpoint: "http://localhost:3100".try_into().unwrap(),
                    host_identifier: format!("node-{id}"),
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
                level: Level::DEBUG,
            },
        }
    }
}

pub fn create_tracing_configs(ids: &[[u8; 32]]) -> Vec<GeneralTracingConfig> {
    ids.iter()
        .enumerate()
        .map(|(_i, _)| {
            #[cfg(feature = "debug")]
            {
                GeneralTracingConfig::local_debug_tracing(_i)
            }
            #[cfg(not(feature = "debug"))]
            {
                GeneralTracingConfig::default()
            }
        })
        .collect()
}

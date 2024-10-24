use nomos_tracing_service::TracingSettings;

#[derive(Clone)]
pub struct GeneralTracingConfig {
    pub tracing_settings: TracingSettings,
}

pub fn create_tracing_configs(ids: &[[u8; 32]]) -> Vec<GeneralTracingConfig> {
    ids.iter()
        .map(|_| GeneralTracingConfig {
            tracing_settings: Default::default(),
        })
        .collect()
}

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalProcessorSettings {
    pub max_delay_seconds: u64,
}

impl Default for TemporalProcessorSettings {
    fn default() -> Self {
        Self {
            max_delay_seconds: 10,
        }
    }
}

// TODO: Implement TemporalProcessor

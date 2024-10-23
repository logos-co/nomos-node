use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TemporalProcessorSettings {
    pub max_delay_seconds: u64,
}

// TODO: Implement TemporalProcessor

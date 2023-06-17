use crate::network::NetworkSettings;
use crate::overlay::OverlaySettings;
use crate::streaming::StreamSettings;
use crate::warding::Ward;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub enum RunnerSettings {
    #[default]
    Sync,
    Async {
        chunks: usize,
    },
    Glauber {
        maximum_iterations: usize,
        update_rate: usize,
    },
    Layered {
        rounds_gap: usize,
        distribution: Option<Vec<f32>>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NodeSettings {
    #[serde(with = "humantime_serde")]
    pub timeout: std::time::Duration,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SimulationSettings {
    #[serde(default)]
    pub wards: Vec<Ward>,
    pub network_settings: NetworkSettings,
    pub overlay_settings: OverlaySettings,
    pub node_settings: NodeSettings,
    #[serde(default)]
    pub runner_settings: RunnerSettings,
    pub stream_settings: StreamSettings,
    #[serde(with = "humantime_serde")]
    pub step_time: std::time::Duration,
    pub node_count: usize,
    pub views_count: usize,
    pub leaders_count: usize,
    pub seed: Option<u64>,
}

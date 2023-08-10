use std::collections::HashMap;

use crate::network::NetworkSettings;
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OverlaySettings {
    #[default]
    Flat,
    Tree(TreeSettings),
    Branch(BranchSettings),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeSettings {
    pub number_of_committees: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BranchSettings {
    pub number_of_levels: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NodeSettings {
    pub network_capacity_kbps: u32,
    #[serde(with = "humantime_serde")]
    pub timeout: std::time::Duration,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SimulationSettings {
    #[serde(default)]
    pub wards: Vec<Ward>,
    #[serde(default)]
    pub record_settings: HashMap<String, bool>,
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

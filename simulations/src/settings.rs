use crate::network::regions::Region;
use crate::node::StepTime;
use crate::warding::Ward;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Default)]
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

#[derive(Deserialize)]
pub struct SimulationSettings<N, O> {
    pub network_behaviors: HashMap<(Region, Region), StepTime>,
    pub regions: Vec<Region>,
    #[serde(default)]
    pub wards: Vec<Ward>,
    pub overlay_settings: O,
    pub node_settings: N,
    pub runner_settings: RunnerSettings,
    pub node_count: usize,
    pub committee_size: usize,
    pub seed: Option<u64>,
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, time::Duration};

    use crate::{network::regions::Region, node::StepTime};

    #[test]
    fn serialize_hashmap() {
        let mut settings: HashMap<(Region, Region), StepTime> = HashMap::new();
        settings.insert((Region::Europe, Region::Europe), Duration::new(1, 1).into());
        let a = serde_json::to_string(&settings);
        println!("{a:?}");
    }
}

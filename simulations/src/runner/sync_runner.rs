use super::SimulationRunner;
use crate::node::Node;
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::warding::SimulationState;
use std::sync::Arc;

/// Simulate with option of dumping the network state as a `::polars::Series`
pub fn simulate<N: Node, O: Overlay>(
    runner: &mut SimulationRunner<N, O>,
    mut out_data: Option<&mut Vec<OutData>>,
) where
    N: Send + Sync,
    N::Settings: Clone,
    O::Settings: Clone,
{
    let state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };

    runner.dump_state_to_out_data(&state, &mut out_data);

    for _ in 1.. {
        runner.step();
        runner.dump_state_to_out_data(&state, &mut out_data);
        // check if any condition makes the simulation stop
        if runner.check_wards(&state) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use crate::{
        network::regions::Region,
        node::dummy::{DummyNode, DummySettings},
        output_processors::OutData,
        overlay::tree::{TreeOverlay, TreeSettings, TreeType},
        runner::SimulationRunner,
        settings::{RunnerSettings, SimulationSettings},
    };

    #[test]
    fn run_dummy_steps() {
        let network_behaviors = HashMap::from([
            (
                (Region::Asia, Region::Asia),
                Duration::from_millis(100).into(),
            ),
            (
                (Region::Asia, Region::Europe),
                Duration::from_millis(500).into(),
            ),
            (
                (Region::Europe, Region::Europe),
                Duration::from_millis(100).into(),
            ),
        ]);
        let regions = vec![Region::Asia, Region::Europe];
        let wards = vec![];
        let overlay_settings = TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            committee_size: 1,
            depth: 1,
        };
        let node_settings = DummySettings {};
        let runner_settings = RunnerSettings::Sync;
        let settings = SimulationSettings {
            network_behaviors,
            regions,
            wards,
            overlay_settings,
            node_settings,
            runner_settings,
            node_count: 10,
            committee_size: 1,
            seed: Some(1),
        };

        let mut out_data: Vec<OutData> = Vec::new();
        let mut runner: SimulationRunner<DummyNode, TreeOverlay> = SimulationRunner::new(settings);
        runner.simulate(Some(&mut out_data));
    }
}

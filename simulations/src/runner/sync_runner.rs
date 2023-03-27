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
    use crate::{
        node::{dummy::DummyNode, Node},
        overlay::tree::TreeOverlay,
        runner::SimulationRunner,
        settings::SimulationSettings,
    };

    #[test]
    fn runner_one_step() {
        let settings = SimulationSettings {
            node_count: 10,
            committee_size: 1,
            ..Default::default()
        };

        let mut runner: SimulationRunner<DummyNode, TreeOverlay> = SimulationRunner::new(settings);
        runner.step();
        let nodes = runner.nodes.read().unwrap();

        for node in nodes.iter() {
            assert_eq!(node.current_view(), 1);
        }
    }
}

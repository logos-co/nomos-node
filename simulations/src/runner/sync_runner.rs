use serde::Serialize;

use super::SimulationRunner;
use crate::node::Node;
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::warding::SimulationState;
use std::sync::Arc;

/// Simulate with option of dumping the network state as a `::polars::Series`
pub fn simulate<M, N: Node, O: Overlay>(
    runner: &mut SimulationRunner<M, N, O>,
    mut out_data: Option<&mut Vec<OutData>>,
) -> anyhow::Result<()>
where
    M: Clone,
    N: Send + Sync,
    N::Settings: Clone,
    N::State: Serialize,
    O::Settings: Clone,
{
    let state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };

    runner.dump_state_to_out_data(&state, &mut out_data)?;

    for _ in 1.. {
        runner.step();
        runner.dump_state_to_out_data(&state, &mut out_data)?;
        // check if any condition makes the simulation stop
        if runner.check_wards(&state) {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        network::{
            behaviour::NetworkBehaviour,
            regions::{Region, RegionsData},
            Network,
        },
        node::{
            dummy::{DummyMessage, DummyNetworkInterface, DummyNode, DummySettings},
            NetworkState, Node, NodeId, SharedState,
        },
        overlay::{
            tree::{TreeOverlay, TreeSettings},
            Overlay,
        },
        runner::SimulationRunner,
        settings::SimulationSettings,
    };
    use crossbeam::channel;
    use rand::rngs::mock::StepRng;
    use std::{
        collections::HashMap,
        sync::{Arc, RwLock},
        time::Duration,
    };

    fn init_network(node_ids: &[NodeId]) -> Network<DummyMessage> {
        let regions = HashMap::from([(Region::Europe, node_ids.to_vec())]);
        let behaviour = HashMap::from([(
            (Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        )]);
        let regions_data = RegionsData::new(regions, behaviour);
        Network::new(regions_data)
    }

    fn init_dummy_nodes(
        node_ids: &[NodeId],
        network: &mut Network<DummyMessage>,
        network_state: SharedState<NetworkState>,
    ) -> Vec<DummyNode> {
        node_ids
            .iter()
            .map(|node_id| {
                let (node_message_sender, node_message_receiver) = channel::unbounded();
                let network_message_receiver = network.connect(*node_id, node_message_receiver);
                let network_interface = DummyNetworkInterface::new(
                    *node_id,
                    node_message_sender,
                    network_message_receiver,
                );
                DummyNode::new(*node_id, network_state.clone(), network_interface)
            })
            .collect()
    }

    #[test]
    fn runner_one_step() {
        let settings: SimulationSettings<DummySettings, TreeSettings> = SimulationSettings {
            node_count: 10,
            committee_size: 1,
            ..Default::default()
        };

        let mut rng = StepRng::new(1, 0);
        let node_ids: Vec<NodeId> = (0..settings.node_count).map(Into::into).collect();
        let overlay = TreeOverlay::new(settings.overlay_settings.clone());
        let mut network = init_network(&node_ids);
        let network_state = Arc::new(RwLock::new(NetworkState {
            layout: overlay.layout(&node_ids, &mut rng),
        }));
        let nodes = init_dummy_nodes(&node_ids, &mut network, network_state);

        let mut runner: SimulationRunner<DummyMessage, DummyNode, TreeOverlay> =
            SimulationRunner::new(network, nodes, settings);
        runner.step();

        let nodes = runner.nodes.read().unwrap();
        for node in nodes.iter() {
            assert_eq!(node.current_view(), 1);
        }
    }

    #[test]
    fn runner_send_receive() {
        let settings: SimulationSettings<DummySettings, TreeSettings> = SimulationSettings {
            node_count: 10,
            committee_size: 1,
            ..Default::default()
        };

        let mut rng = StepRng::new(1, 0);
        let node_ids: Vec<NodeId> = (0..settings.node_count).map(Into::into).collect();
        let overlay = TreeOverlay::new(settings.overlay_settings.clone());
        let mut network = init_network(&node_ids);
        let network_state = Arc::new(RwLock::new(NetworkState {
            layout: overlay.layout(&node_ids, &mut rng),
        }));
        let nodes = init_dummy_nodes(&node_ids, &mut network, network_state);

        for node in nodes.iter() {
            // All nodes send one message to NodeId(1).
            // Nodes can sent messages to themselves.
            node.send_message(node_ids[1], DummyMessage::EventOne(42));
        }
        network.collect_messages();

        let mut runner: SimulationRunner<DummyMessage, DummyNode, TreeOverlay> =
            SimulationRunner::new(network, nodes, settings);

        runner.step();

        let nodes = runner.nodes.read().unwrap();
        let state = nodes[1].state();
        assert_eq!(state.event_one_count, 10);
    }
}

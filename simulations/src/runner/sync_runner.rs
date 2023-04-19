use serde::Serialize;

use super::{SimulationRunner, SimulationRunnerInner, SimulationRunnerHandle};
use crate::node::Node;
use crate::overlay::Overlay;
use crate::streaming::{Producer, Subscriber};
use crate::warding::SimulationState;
use std::sync::Arc;
use crossbeam::channel::{bounded, select};

/// Simulate with sending the network state to any subscriber
pub fn simulate<M, N: Node, O: Overlay, P: Producer>(
    runner: &mut SimulationRunner<M, N, O, P>,
    settings: P::Settings,
) -> anyhow::Result<SimulationRunnerHandle>
where
    M: Clone + Send,
    N: Send + Sync,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O::Settings: Clone,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        Send + Sync + 'static + for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
{
    let state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };

    let p = P::new(settings)?;
    scopeguard::defer!(if let Err(e) = p.stop() {
        eprintln!("Error stopping producer: {e}");
    });
    let subscriber = p.subscribe()?;
    std::thread::spawn(move || {
        if let Err(e) = subscriber.run() {
            eprintln!("Error in subscriber: {e}");
        }
    });
    let mut inner = runner.inner.clone();
    let nodes = runner.nodes.clone();
    p.send(<P::Subscriber as Subscriber>::Record::try_from(&state)?)?;
    let (stop_tx, stop_rx) = bounded(1);
    let handle = SimulationRunnerHandle {
        stop_tx,
        handle: std::thread::spawn(move || {
            loop {
                select! {
                    recv(stop_rx) -> _ => {
                        return Ok(());
                    }
                    default => {
                        let nodes = nodes.write().expect("Write access to nodes vector");
                        inner.step(&mut nodes);
                        p.send(<P::Subscriber as Subscriber>::Record::try_from(&state).unwrap()).unwrap();
                        // check if any condition makes the simulation stop
                        if inner.check_wards(&state) {
                            return Ok(());
                        }
                    }
                }
            }
        }),
    };
    Ok(handle)
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
            Node, NodeId, OverlayState, SharedState, ViewOverlay,
        },
        output_processors::OutData,
        overlay::{
            tree::{TreeOverlay, TreeSettings},
            Overlay,
        },
        runner::SimulationRunner,
        settings::SimulationSettings,
        streaming::naive::{NaiveProducer, NaiveSettings},
    };
    use crossbeam::channel;
    use rand::rngs::mock::StepRng;
    use std::{
        collections::{BTreeMap, HashMap},
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
        overlay_state: SharedState<OverlayState>,
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
                DummyNode::new(*node_id, 0, overlay_state.clone(), network_interface)
            })
            .collect()
    }

    #[test]
    fn runner_one_step() {
        let settings: SimulationSettings<DummySettings, TreeSettings, NaiveSettings> =
            SimulationSettings {
                node_count: 10,
                committee_size: 1,
                ..Default::default()
            };

        let mut rng = StepRng::new(1, 0);
        let node_ids: Vec<NodeId> = (0..settings.node_count).map(Into::into).collect();
        let overlay = TreeOverlay::new(settings.overlay_settings.clone());
        let mut network = init_network(&node_ids);
        let view = ViewOverlay {
            leaders: overlay.leaders(&node_ids, 1, &mut rng).collect(),
            layout: overlay.layout(&node_ids, &mut rng),
        };
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            all_nodes: node_ids.clone(),
            overlays: BTreeMap::from([(0, view.clone()), (1, view)]),
        }));
        let nodes = init_dummy_nodes(&node_ids, &mut network, overlay_state);

        let mut runner: SimulationRunner<
            DummyMessage,
            DummyNode,
            TreeOverlay,
            NaiveProducer<OutData>,
        > = SimulationRunner::new(network, nodes, settings);
        runner.step();

        let nodes = runner.nodes.read().unwrap();
        for node in nodes.iter() {
            assert_eq!(node.current_view(), 0);
        }
    }

    #[test]
    fn runner_send_receive() {
        let settings: SimulationSettings<DummySettings, TreeSettings, NaiveSettings> =
            SimulationSettings {
                node_count: 10,
                committee_size: 1,
                ..Default::default()
            };

        let mut rng = StepRng::new(1, 0);
        let node_ids: Vec<NodeId> = (0..settings.node_count).map(Into::into).collect();
        let overlay = TreeOverlay::new(settings.overlay_settings.clone());
        let mut network = init_network(&node_ids);
        let view = ViewOverlay {
            leaders: overlay.leaders(&node_ids, 1, &mut rng).collect(),
            layout: overlay.layout(&node_ids, &mut rng),
        };
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            all_nodes: node_ids.clone(),
            overlays: BTreeMap::from([
                (0, view.clone()),
                (1, view.clone()),
                (42, view.clone()),
                (43, view),
            ]),
        }));
        let nodes = init_dummy_nodes(&node_ids, &mut network, overlay_state);

        for node in nodes.iter() {
            // All nodes send one message to NodeId(1).
            // Nodes can send messages to themselves.
            node.send_message(node_ids[1], DummyMessage::Proposal(42.into()));
        }
        network.collect_messages();

        let mut runner: SimulationRunner<
            DummyMessage,
            DummyNode,
            TreeOverlay,
            NaiveProducer<OutData>,
        > = SimulationRunner::new(network, nodes, settings);

        runner.step();

        let nodes = runner.nodes.read().unwrap();
        let state = nodes[1].state();
        assert_eq!(state.message_count, 10);
    }
}

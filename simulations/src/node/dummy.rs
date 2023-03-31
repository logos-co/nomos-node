// std
// crates
use crossbeam::channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
// internal
use crate::{
    network::{NetworkInterface, NetworkMessage},
    node::{Node, NodeId},
};

use super::{OverlayState, SharedState};

#[derive(Debug, Default, Serialize)]
pub struct DummyState {
    pub current_view: usize,
    pub event_one_count: usize,
}

#[derive(Clone, Default, Deserialize)]
pub struct DummySettings {}

#[derive(Clone)]
pub enum DummyMessage {
    EventOne(usize),
    EventTwo(usize),
}

pub struct DummyNode {
    node_id: NodeId,
    state: DummyState,
    settings: DummySettings,
    overlay_state: SharedState<OverlayState>,
    network_interface: DummyNetworkInterface,
}

impl DummyNode {
    pub fn new(
        node_id: NodeId,
        overlay_state: SharedState<OverlayState>,
        network_interface: DummyNetworkInterface,
    ) -> Self {
        Self {
            node_id,
            state: Default::default(),
            settings: Default::default(),
            overlay_state,
            network_interface,
        }
    }

    pub fn send_message(&self, address: NodeId, message: DummyMessage) {
        self.network_interface.send_message(address, message);
    }
}

impl Node for DummyNode {
    type Settings = DummySettings;
    type State = DummyState;

    fn id(&self) -> NodeId {
        self.node_id
    }

    fn current_view(&self) -> usize {
        self.state.current_view
    }

    fn state(&self) -> &DummyState {
        &self.state
    }

    fn step(&mut self) {
        let incoming_messages = self.network_interface.receive_messages();
        self.state.current_view += 1;

        for message in incoming_messages {
            match message.payload {
                DummyMessage::EventOne(_) => self.state.event_one_count += 1,
                DummyMessage::EventTwo(_) => todo!(),
            }
        }
    }
}

pub struct DummyNetworkInterface {
    id: NodeId,
    sender: Sender<NetworkMessage<DummyMessage>>,
    receiver: Receiver<NetworkMessage<DummyMessage>>,
}

impl DummyNetworkInterface {
    pub fn new(
        id: NodeId,
        sender: Sender<NetworkMessage<DummyMessage>>,
        receiver: Receiver<NetworkMessage<DummyMessage>>,
    ) -> Self {
        Self {
            id,
            sender,
            receiver,
        }
    }
}

impl NetworkInterface for DummyNetworkInterface {
    type Payload = DummyMessage;

    fn send_message(&self, address: NodeId, message: Self::Payload) {
        let message = NetworkMessage::new(self.id, address, message);
        self.sender.send(message).unwrap();
    }

    fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
        self.receiver.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, RwLock},
    };

    use rand::rngs::mock::StepRng;

    use crate::{
        node::{NodeId, OverlayState, View},
        overlay::{
            tree::{TreeOverlay, TreeSettings},
            Overlay,
        },
    };

    #[test]
    fn get_tree_overlay() {
        let mut rng = StepRng::new(1, 0);
        let node_ids: Vec<NodeId> = (0..10).map(Into::into).collect();

        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: Default::default(),
            depth: 3,
            committee_size: 1,
        });
        let view = View {
            //leaders: overlay.leaders(&node_ids, 3, &mut rng).collect(),
            leaders: vec![0.into(), 1.into(), 2.into()],
            layout: overlay.layout(&node_ids, &mut rng),
        };
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            views: BTreeMap::from([(1, view.clone())]),
        }));
    }
}

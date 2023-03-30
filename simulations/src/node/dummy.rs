// std
// crates
use crossbeam::channel::{Receiver, Sender};
use serde::Deserialize;
// internal
use crate::{
    network::{NetworkInterface, NetworkMessage},
    node::{Node, NodeId},
};

use super::{NetworkState, SharedState};

#[derive(Debug, Default)]
pub struct DummyState {
    pub current_view: usize,
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
    _settings: DummySettings,
    _network_state: SharedState<NetworkState>,
    network_interface: DummyNetworkInterface,
}

impl DummyNode {
    pub fn new(
        node_id: NodeId,
        _network_state: SharedState<NetworkState>,
        network_interface: DummyNetworkInterface,
    ) -> Self {
        Self {
            node_id,
            state: DummyState { current_view: 0 },
            _settings: DummySettings {},
            _network_state,
            network_interface,
        }
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
                DummyMessage::EventOne(_) => todo!(),
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

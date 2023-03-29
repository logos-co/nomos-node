// std
use std::sync::mpsc::{Receiver, Sender};
// crates
use rand::Rng;
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

pub enum DummyMessage {
    SendTo(NodeId),
    MyView(usize),
}

pub struct DummyNode {
    id: NodeId,
    state: DummyState,
    _settings: DummySettings,
    _network_state: SharedState<NetworkState>,
    _network_interface: DummyNetworkInterface,
}

impl Node for DummyNode {
    type Settings = DummySettings;
    type State = DummyState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.state.current_view
    }

    fn state(&self) -> &DummyState {
        &self.state
    }

    fn step(&mut self) {
        let incoming_messages = self._network_interface.receive_messages();
        self.state.current_view += 1;

        for message in incoming_messages {
            match message.payload {
                DummyMessage::SendTo(_) => todo!(),
                DummyMessage::MyView(_) => todo!(),
            }
        }
    }
}

pub struct DummyNetworkInterface {}

impl DummyNetworkInterface {
    pub fn new(
        local_addr: NodeId,
        inbound: Sender<NetworkMessage<DummyMessage>>,
        outbound: Receiver<NetworkMessage<DummyMessage>>,
    ) -> Self {
        todo!()
    }
}

impl NetworkInterface for DummyNetworkInterface {
    type Payload = DummyMessage;

    fn send_message(&mut self, address: usize, message: Self::Payload) {
        todo!()
    }

    fn receive_messages(&mut self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
        todo!()
    }
}

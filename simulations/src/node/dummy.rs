// std
use std::sync::mpsc::{Receiver, Sender};
// crates
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
    SendTo(NodeId),
    MyView(usize),
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
                DummyMessage::SendTo(_) => todo!(),
                DummyMessage::MyView(_) => todo!(),
            }
        }
    }
}

pub struct DummyNetworkInterface {}

impl DummyNetworkInterface {
    pub fn new(
        _local_addr: NodeId,
        _sender: Sender<NetworkMessage<DummyMessage>>,
        _receiver: Receiver<NetworkMessage<DummyMessage>>,
    ) -> Self {
        todo!()
    }

    pub fn get_outgoing_channel() -> Receiver<DummyMessage> {
        todo!()
    }
}

impl NetworkInterface for DummyNetworkInterface {
    type Payload = DummyMessage;

    fn send_message(&self, _address: NodeId, _message: Self::Payload) {
        todo!()
    }

    fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
        todo!()
    }
}

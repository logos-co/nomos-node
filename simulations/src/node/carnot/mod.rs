mod event_builder;
mod messages;

use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};

// std
// crates
use self::{event_builder::EventBuilderSettings, messages::CarnotMessage};
use serde::{Deserialize, Serialize};

// internal
use super::{Node, NodeId};

#[derive(Default, Serialize)]
pub struct CarnotState {}

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode {
    id: NodeId,
    state: CarnotState,
    settings: CarnotSettings,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    event_builder: event_builder::EventBuilder,
}

impl CarnotNode {
    pub fn new(id: NodeId, settings: CarnotSettings) -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        Self {
            id,
            state: Default::default(),
            settings,
            network_interface: InMemoryNetworkInterface::new(id, sender, receiver),
            event_builder: Default::default(),
        }
    }

    pub fn send_message(&self, message: NetworkMessage<CarnotMessage>) {
        self.network_interface.send_message(self.id, message);
    }
}

impl Node for CarnotNode {
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.event_builder.current_view
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self) {
        self.event_builder.step(
            self.network_interface
                .receive_messages()
                .into_iter()
                .map(|m| m.payload),
        );
    }
}

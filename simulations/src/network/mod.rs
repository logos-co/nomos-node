// std
use std::{collections::HashMap, time::Duration};
// crates
use rand::Rng;
// internal
use crate::node::{Node, NodeId};

pub mod behaviour;
pub mod regions;

#[derive(Clone)]
pub struct Network {
    pub regions: regions::RegionsData,
    node_messages: HashMap<NodeId, Vec<NetworkMessage<String>>>,
}

impl Network {
    pub fn new(regions: regions::RegionsData) -> Self {
        Self {
            regions,
            node_messages: HashMap::new(),
        }
    }

    pub fn send_message_cost<R: Rng>(
        &self,
        rng: &mut R,
        node_a: NodeId,
        node_b: NodeId,
    ) -> Option<Duration> {
        let network_behaviour = self.regions.network_behaviour(node_a, node_b);
        (!network_behaviour.should_drop(rng))
            // TODO: use a delay range
            .then(|| network_behaviour.delay())
    }
}

#[derive(Clone)]
pub struct NetworkMessage<M> {
    pub from: usize,
    pub to: usize,
    pub payload: M,
}

pub struct NodeMessage {}

impl NetworkInterface for Network {
    type Payload = ();

    fn send_message(&mut self, address: usize, message: Self::Payload) {
        todo!()
    }

    fn receive_messages(&mut self) -> Vec<NetworkMessage<Self::Payload>> {
        todo!()
    }
}

pub trait NetworkInterface: Send + Sync {
    type Payload;

    fn send_message(&mut self, address: usize, message: Self::Payload);
    fn receive_messages(&mut self) -> Vec<NetworkMessage<Self::Payload>>;
}

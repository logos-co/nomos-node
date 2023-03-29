// std
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};
// crates
use rand::Rng;
// internal
use crate::node::{Node, NodeId};

pub mod behaviour;
pub mod regions;

#[derive(Clone)]
pub struct Network<M> {
    pub regions: regions::RegionsData,
    node_messages: HashMap<NodeId, Vec<NetworkMessage<M>>>,
}

impl<M> Network<M> {
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

    pub fn connect(
        &mut self,
        node_id: NodeId,
        node_message_receiver: Receiver<NetworkMessage<M>>,
    ) -> Receiver<NetworkMessage<M>> {
        todo!()
    }
}

#[derive(Clone)]
pub struct NetworkMessage<M> {
    pub from: usize,
    pub to: usize,
    pub payload: M,
}

pub trait NetworkInterface: Send + Sync {
    type Payload;

    fn send_message(&mut self, address: usize, message: Self::Payload);
    fn receive_messages(&mut self) -> Vec<NetworkMessage<Self::Payload>>;
}

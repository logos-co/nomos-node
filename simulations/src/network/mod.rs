// std
use std::time::Duration;
// crates
use rand::Rng;
// internal
use crate::node::NodeId;

pub mod behaviour;
pub mod regions;

pub struct Network {
    pub regions: regions::RegionsData,
}

impl Network {
    pub fn new(regions: regions::RegionsData) -> Self {
        Self { regions }
    }

    pub fn send_message_cost<R: Rng>(
        &self,
        rng: &mut R,
        node_a: NodeId,
        node_b: NodeId,
    ) -> Option<Duration> {
        let network_behaviour = self.regions.network_behaviour(node_a, node_b);
        network_behaviour
            .should_drop(rng)
            // TODO: use a delay range
            .then(|| network_behaviour.delay())
    }
}

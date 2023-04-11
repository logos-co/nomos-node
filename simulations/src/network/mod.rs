// std
use std::{
    collections::HashMap,
    ops::Add,
    time::{Duration, Instant},
};
// crates
use crossbeam::channel::{self, Receiver, Sender};
use rand::{rngs::ThreadRng, Rng};
use rayon::prelude::*;
// internal
use crate::node::NodeId;

pub mod behaviour;
pub mod regions;

type NetworkTime = Instant;

pub struct Network<M> {
    pub regions: regions::RegionsData,
    network_time: NetworkTime,
    messages: Vec<(NetworkTime, NetworkMessage<M>)>,
    from_node_receivers: HashMap<NodeId, Receiver<NetworkMessage<M>>>,
    to_node_senders: HashMap<NodeId, Sender<NetworkMessage<M>>>,
}

impl<M> Network<M>
where
    M: Send + Sync + Clone,
{
    pub fn new(regions: regions::RegionsData) -> Self {
        Self {
            regions,
            network_time: Instant::now(),
            messages: Vec::new(),
            from_node_receivers: HashMap::new(),
            to_node_senders: HashMap::new(),
        }
    }

    fn send_message_cost<R: Rng>(
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
        let (to_node_sender, from_network_receiver) = channel::unbounded();
        self.from_node_receivers
            .insert(node_id, node_message_receiver);
        self.to_node_senders.insert(node_id, to_node_sender);
        from_network_receiver
    }

    /// Collects and dispatches messages to connected interfaces.
    pub fn step(&mut self, time_passed: Duration) {
        self.collect_messages();
        self.dispatch_after(time_passed);
    }

    /// Receive and store all messages from nodes.
    pub fn collect_messages(&mut self) {
        let mut new_messages = self
            .from_node_receivers
            .par_iter()
            .flat_map(|(_, from_node)| {
                from_node
                    .try_iter()
                    .map(|msg| (self.network_time, msg))
                    .collect::<Vec<_>>()
            })
            .collect();

        self.messages.append(&mut new_messages);
    }

    /// Reiterate all messages and send to appropriate nodes if simulated
    /// delay has passed.
    pub fn dispatch_after(&mut self, time_passed: Duration) {
        self.network_time += time_passed;

        let delayed = self
            .messages
            .par_iter()
            .filter(|(network_time, message)| {
                let mut rng = ThreadRng::default();
                self.send_or_drop_message(&mut rng, network_time, message)
            })
            .cloned()
            .collect();

        self.messages = delayed;
    }

    /// Returns true if message needs to be delayed and be dispatched in future.
    fn send_or_drop_message<R: Rng>(
        &self,
        rng: &mut R,
        network_time: &NetworkTime,
        message: &NetworkMessage<M>,
    ) -> bool {
        if let Some(delay) = self.send_message_cost(rng, message.from, message.to) {
            if network_time.add(delay) <= self.network_time {
                let to_node = self.to_node_senders.get(&message.to).unwrap();
                to_node
                    .send(message.clone())
                    .expect("Node should have connection");
                return false;
            } else {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug)]
pub struct NetworkMessage<M> {
    pub from: NodeId,
    pub to: NodeId,
    pub payload: M,
}

impl<M> NetworkMessage<M> {
    pub fn new(from: NodeId, to: NodeId, payload: M) -> Self {
        Self { from, to, payload }
    }
}

pub trait NetworkInterface {
    type Payload;

    fn send_message(&self, address: NodeId, message: Self::Payload);
    fn receive_messages(&self) -> Vec<NetworkMessage<Self::Payload>>;
}

#[cfg(test)]
mod tests {
    use super::{
        behaviour::NetworkBehaviour,
        regions::{Region, RegionsData},
        Network, NetworkInterface, NetworkMessage,
    };
    use crate::node::NodeId;
    use crossbeam::channel::{self, Receiver, Sender};
    use std::{collections::HashMap, time::Duration};

    struct MockNetworkInterface {
        id: NodeId,
        sender: Sender<NetworkMessage<()>>,
        receiver: Receiver<NetworkMessage<()>>,
    }

    impl MockNetworkInterface {
        pub fn new(
            id: NodeId,
            sender: Sender<NetworkMessage<()>>,
            receiver: Receiver<NetworkMessage<()>>,
        ) -> Self {
            Self {
                id,
                sender,
                receiver,
            }
        }
    }

    impl NetworkInterface for MockNetworkInterface {
        type Payload = ();

        fn send_message(&self, address: NodeId, message: Self::Payload) {
            let message = NetworkMessage::new(self.id, address, message);
            self.sender.send(message).unwrap();
        }

        fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
            self.receiver.try_iter().collect()
        }
    }

    #[test]
    fn send_receive_messages() {
        let node_a = 0.into();
        let node_b = 1.into();

        let regions = HashMap::from([(Region::Europe, vec![node_a, node_b])]);
        let behaviour = HashMap::from([(
            (Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        )]);
        let regions_data = RegionsData::new(regions, behaviour);
        let mut network = Network::new(regions_data);

        let (from_a_sender, from_a_receiver) = channel::unbounded();
        let to_a_receiver = network.connect(node_a, from_a_receiver);
        let a = MockNetworkInterface::new(node_a, from_a_sender, to_a_receiver);

        let (from_b_sender, from_b_receiver) = channel::unbounded();
        let to_b_receiver = network.connect(node_b, from_b_receiver);
        let b = MockNetworkInterface::new(node_b, from_b_sender, to_b_receiver);

        a.send_message(node_b, ());
        network.collect_messages();

        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 0);

        network.step(Duration::from_millis(0));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 0);

        network.step(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 1);

        network.step(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 0);

        b.send_message(node_a, ());
        b.send_message(node_a, ());
        b.send_message(node_a, ());
        network.collect_messages();

        network.dispatch_after(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 3);
        assert_eq!(b.receive_messages().len(), 0);
    }

    #[test]
    fn regions_send_receive_messages() {
        let node_a = 0.into();
        let node_b = 1.into();
        let node_c = 2.into();

        let regions = HashMap::from([
            (Region::Asia, vec![node_a, node_b]),
            (Region::Europe, vec![node_c]),
        ]);
        let behaviour = HashMap::from([
            (
                (Region::Asia, Region::Asia),
                NetworkBehaviour::new(Duration::from_millis(100), 0.0),
            ),
            (
                (Region::Asia, Region::Europe),
                NetworkBehaviour::new(Duration::from_millis(500), 0.0),
            ),
            (
                (Region::Europe, Region::Europe),
                NetworkBehaviour::new(Duration::from_millis(100), 0.0),
            ),
        ]);
        let regions_data = RegionsData::new(regions, behaviour);
        let mut network = Network::new(regions_data);

        let (from_a_sender, from_a_receiver) = channel::unbounded();
        let to_a_receiver = network.connect(node_a, from_a_receiver);
        let a = MockNetworkInterface::new(node_a, from_a_sender, to_a_receiver);

        let (from_b_sender, from_b_receiver) = channel::unbounded();
        let to_b_receiver = network.connect(node_b, from_b_receiver);
        let b = MockNetworkInterface::new(node_b, from_b_sender, to_b_receiver);

        let (from_c_sender, from_c_receiver) = channel::unbounded();
        let to_c_receiver = network.connect(node_c, from_c_receiver);
        let c = MockNetworkInterface::new(node_c, from_c_sender, to_c_receiver);

        a.send_message(node_b, ());
        a.send_message(node_c, ());
        network.collect_messages();

        b.send_message(node_a, ());
        b.send_message(node_c, ());
        network.collect_messages();

        c.send_message(node_a, ());
        c.send_message(node_b, ());
        network.collect_messages();

        network.dispatch_after(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 1);
        assert_eq!(b.receive_messages().len(), 1);
        assert_eq!(c.receive_messages().len(), 0);

        a.send_message(node_b, ());
        b.send_message(node_c, ());
        network.collect_messages();

        network.dispatch_after(Duration::from_millis(400));
        assert_eq!(a.receive_messages().len(), 1); // c to a
        assert_eq!(b.receive_messages().len(), 2); // c to b && a to b
        assert_eq!(c.receive_messages().len(), 2); // a to c && b to c

        network.dispatch_after(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 0);
        assert_eq!(c.receive_messages().len(), 1); // b to c
    }
}

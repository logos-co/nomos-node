// std
use std::{
    collections::HashMap,
    ops::Add,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};
// crates
use rand::Rng;
// internal
use crate::node::NodeId;

pub mod behaviour;
pub mod regions;

pub type NetworkTime = Instant;

pub struct Network<M> {
    pub regions: regions::RegionsData,
    network_time: NetworkTime,
    messages: Vec<(NetworkTime, NetworkMessage<M>)>,
    from_node_receivers: HashMap<NodeId, Receiver<NetworkMessage<M>>>,
    to_node_senders: HashMap<NodeId, Sender<NetworkMessage<M>>>,
}

impl<M> Network<M>
where
    M: Clone,
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
        let (to_node_sender, from_network_receiver) = mpsc::channel();
        self.from_node_receivers
            .insert(node_id, node_message_receiver);
        self.to_node_senders.insert(node_id, to_node_sender);
        from_network_receiver
    }

    pub fn step<R: Rng>(&mut self, rng: &mut R, time_passed: Duration) {
        self.network_time += time_passed;

        // Receive and store all messages from nodes.
        self.from_node_receivers.iter().for_each(|(_, from_node)| {
            while let Ok(message) = from_node.try_recv() {
                self.messages.push((self.network_time, message));
            }
        });

        // Reiterate all messages and send to appropriate nodes if simulated
        // delay has passed.
        if let Some((network_time, message)) = self.messages.pop() {
            // TODO: Handle message drops (remove unwrap).
            let delay = self
                .send_message_cost(rng, message.from, message.to)
                .unwrap();
            if network_time.add(delay) <= self.network_time {
                let to_node = self.to_node_senders.get(&message.to).unwrap();
                to_node.send(message).expect("Node should have connection");
            } else {
                self.messages.push((network_time, message));
            }
        }
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
    use rand::rngs::mock::StepRng;

    use super::{
        behaviour::NetworkBehaviour,
        regions::{Region, RegionsData},
        Network, NetworkInterface, NetworkMessage,
    };
    use crate::node::NodeId;
    use std::{
        collections::HashMap,
        sync::mpsc::{self, Receiver, Sender},
        time::Duration,
    };

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
            let mut messages = vec![];
            while let Ok(message) = self.receiver.try_recv() {
                messages.push(message);
            }
            messages
        }
    }

    #[test]
    fn send_receive_messages() {
        let mut rng = StepRng::new(1, 0);
        let node_a = 0.into();
        let node_b = 1.into();

        let regions = HashMap::from([(Region::Europe, vec![node_a, node_b])]);
        let behaviour = HashMap::from([(
            (Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        )]);
        let regions_data = RegionsData::new(regions, behaviour);
        let mut network = Network::new(regions_data);

        let (from_a_sender, from_a_receiver) = mpsc::channel();
        let to_a_receiver = network.connect(node_a, from_a_receiver);
        let a = MockNetworkInterface::new(node_a, from_a_sender, to_a_receiver);

        let (from_b_sender, from_b_receiver) = mpsc::channel();
        let to_b_receiver = network.connect(node_b, from_b_receiver);
        let b = MockNetworkInterface::new(node_b, from_b_sender, to_b_receiver);

        a.send_message(node_b, ());

        // Currently messages are received during the network step.
        network.step(&mut rng, Duration::from_millis(0));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 0);

        network.step(&mut rng, Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 1);

        network.step(&mut rng, Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 0);
    }
}

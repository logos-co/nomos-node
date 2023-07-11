// std
use std::{
    collections::HashMap,
    ops::Add,
    str::FromStr,
    time::{Duration, Instant},
};
// crates
use crossbeam::channel::{self, Receiver, Sender};
use rand::{rngs::ThreadRng, Rng};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
// internal
use crate::node::NodeId;

pub mod behaviour;
pub mod regions;

type NetworkTime = Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NetworkBehaviourKey {
    pub from: regions::Region,
    pub to: regions::Region,
}

impl NetworkBehaviourKey {
    pub fn new(from: regions::Region, to: regions::Region) -> Self {
        Self { from, to }
    }
}

impl Serialize for NetworkBehaviourKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let s = format!("{}:{}", self.from, self.to);
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for NetworkBehaviourKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        let mut split = s.split(':');
        let from = split.next().ok_or(serde::de::Error::custom(
            "NetworkBehaviourKey should be in the form of `from_region:to_region`",
        ))?;
        let to = split.next().ok_or(serde::de::Error::custom(
            "NetworkBehaviourKey should be in the form of `from_region:to_region`",
        ))?;
        Ok(Self::new(
            regions::Region::from_str(from).map_err(serde::de::Error::custom)?,
            regions::Region::from_str(to).map_err(serde::de::Error::custom)?,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct NetworkSettings {
    #[serde(with = "network_behaviors_serde")]
    pub network_behaviors: HashMap<NetworkBehaviourKey, Duration>,
    /// Represents node distribution in the simulated regions.
    /// The sum of distributions should be 1.
    pub regions: HashMap<regions::Region, f32>,
}

/// Ser/Deser `HashMap<NetworkBehaviourKey, Duration>` to humantime format.
mod network_behaviors_serde {
    use super::{Deserialize, Duration, HashMap, NetworkBehaviourKey};

    /// Have to implement this manually because of the `serde_json` will panic if the key of map
    /// is not a string.
    pub fn serialize<S>(
        vals: &HashMap<NetworkBehaviourKey, Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut ser = serializer.serialize_map(Some(vals.len()))?;
        for (k, v) in vals {
            ser.serialize_key(&k)?;
            ser.serialize_value(&humantime::format_duration(*v).to_string())?;
        }
        ser.end()
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<NetworkBehaviourKey, Duration>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map = HashMap::<NetworkBehaviourKey, String>::deserialize(deserializer)?;
        map.into_iter()
            .map(|(k, v)| {
                let v = humantime::parse_duration(&v).map_err(serde::de::Error::custom)?;
                Ok((k, v))
            })
            .collect::<Result<HashMap<_, _>, _>>()
    }
}

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
        match message {
            NetworkMessage::Adhoc(msg) => {
                let recipient = msg.to.expect("Adhoc message has recipient");
                let to_node = self.to_node_senders.get(&recipient).unwrap();
                self.send_delayed(rng, recipient, to_node, network_time, msg)
            }
            NetworkMessage::Broadcast(msg) => {
                let mut adhoc = msg.clone();
                for (recipient, to_node) in self.to_node_senders.iter() {
                    adhoc.to = Some(*recipient);
                    self.send_delayed(rng, *recipient, to_node, network_time, &adhoc);
                }
                false
            }
        }
    }

    fn send_delayed<R: Rng>(
        &self,
        rng: &mut R,
        to: NodeId,
        to_node: &Sender<NetworkMessage<M>>,
        network_time: &NetworkTime,
        msg: &AdhocMessage<M>,
    ) -> bool {
        if let Some(delay) = self.send_message_cost(rng, msg.from, to) {
            if network_time.add(delay) <= self.network_time {
                to_node
                    .send(NetworkMessage::Adhoc(msg.clone()))
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
pub struct AdhocMessage<M> {
    pub from: NodeId,
    pub to: Option<NodeId>,
    pub payload: M,
}

#[derive(Clone, Debug)]
pub enum NetworkMessage<M> {
    Adhoc(AdhocMessage<M>),
    Broadcast(AdhocMessage<M>),
}

impl<M> NetworkMessage<M> {
    pub fn adhoc(from: NodeId, to: NodeId, payload: M) -> Self {
        Self::Adhoc(AdhocMessage {
            from,
            to: Some(to),
            payload,
        })
    }

    pub fn broadcast(from: NodeId, payload: M) -> Self {
        Self::Broadcast(AdhocMessage {
            from,
            to: None,
            payload,
        })
    }

    pub fn get_payload(self) -> M {
        match self {
            NetworkMessage::Adhoc(AdhocMessage { payload, .. }) => payload,
            NetworkMessage::Broadcast(AdhocMessage { payload, .. }) => payload,
        }
    }
}

pub trait NetworkInterface {
    type Payload;

    fn broadcast(&self, message: Self::Payload);
    fn send_message(&self, address: NodeId, message: Self::Payload);
    fn receive_messages(&self) -> Vec<NetworkMessage<Self::Payload>>;
}

pub struct InMemoryNetworkInterface<M> {
    id: NodeId,
    sender: Sender<NetworkMessage<M>>,
    receiver: Receiver<NetworkMessage<M>>,
}

impl<M> InMemoryNetworkInterface<M> {
    pub fn new(
        id: NodeId,
        sender: Sender<NetworkMessage<M>>,
        receiver: Receiver<NetworkMessage<M>>,
    ) -> Self {
        Self {
            id,
            sender,
            receiver,
        }
    }
}

impl<M> NetworkInterface for InMemoryNetworkInterface<M> {
    type Payload = M;

    fn broadcast(&self, message: Self::Payload) {
        let message = NetworkMessage::broadcast(self.id, message);
        self.sender.send(message).unwrap();
    }

    fn send_message(&self, address: NodeId, message: Self::Payload) {
        let message = NetworkMessage::adhoc(self.id, address, message);
        self.sender.send(message).unwrap();
    }

    fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
        self.receiver.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        behaviour::NetworkBehaviour,
        regions::{Region, RegionsData},
        Network, NetworkInterface, NetworkMessage,
    };
    use crate::{
        network::NetworkBehaviourKey,
        node::{NodeId, NodeIdExt},
    };
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

        fn broadcast(&self, message: Self::Payload) {
            let message = NetworkMessage::broadcast(self.id, message);
            self.sender.send(message).unwrap();
        }

        fn send_message(&self, address: NodeId, message: Self::Payload) {
            let message = NetworkMessage::adhoc(self.id, address, message);
            self.sender.send(message).unwrap();
        }

        fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
            self.receiver.try_iter().collect()
        }
    }

    #[test]
    fn send_receive_messages() {
        let node_a = NodeId::from_index(0);
        let node_b = NodeId::from_index(1);

        let regions = HashMap::from([(Region::Europe, vec![node_a, node_b])]);
        let behaviour = HashMap::from([(
            NetworkBehaviourKey::new(Region::Europe, Region::Europe),
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
        let node_a = NodeId::from_index(0);
        let node_b = NodeId::from_index(1);
        let node_c = NodeId::from_index(2);

        let regions = HashMap::from([
            (Region::Asia, vec![node_a, node_b]),
            (Region::Europe, vec![node_c]),
        ]);
        let behaviour = HashMap::from([
            (
                NetworkBehaviourKey::new(Region::Asia, Region::Asia),
                NetworkBehaviour::new(Duration::from_millis(100), 0.0),
            ),
            (
                NetworkBehaviourKey::new(Region::Asia, Region::Europe),
                NetworkBehaviour::new(Duration::from_millis(500), 0.0),
            ),
            (
                NetworkBehaviourKey::new(Region::Europe, Region::Europe),
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

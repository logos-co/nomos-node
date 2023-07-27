// std
use std::{
    collections::HashMap,
    ops::Add,
    str::FromStr,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
// crates
use crossbeam::channel::{self, Receiver, Sender};
use parking_lot::Mutex;
use rand::{rngs::SmallRng, Rng, SeedableRng};
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

/// Represents node network capacity and current load in bytes.
struct NodeNetworkCapacity {
    capacity_bps: u32,
    current_load: Mutex<u32>,
    load_to_flush: AtomicU32,
}

impl NodeNetworkCapacity {
    fn new(capacity_bps: u32) -> Self {
        Self {
            capacity_bps,
            current_load: Mutex::new(0),
            load_to_flush: AtomicU32::new(0),
        }
    }

    fn increase_load(&self, load: u32) -> bool {
        let mut current_load = self.current_load.lock();
        if *current_load + load <= self.capacity_bps {
            *current_load += load;
            true
        } else {
            false
        }
    }

    fn decrease_load(&self, load: u32) {
        self.load_to_flush.fetch_add(load, Ordering::Relaxed);
    }

    fn flush_load(&self) {
        let mut s = self.current_load.lock();
        *s -= self.load_to_flush.load(Ordering::Relaxed);
        self.load_to_flush.store(0, Ordering::Relaxed);
    }
}

pub struct Network<M: std::fmt::Debug> {
    pub regions: regions::RegionsData,
    network_time: NetworkTime,
    messages: Vec<(NetworkTime, NetworkMessage<M>)>,
    node_network_capacity: HashMap<NodeId, NodeNetworkCapacity>,
    from_node_receivers: HashMap<NodeId, Receiver<NetworkMessage<M>>>,
    from_node_broadcast_receivers: HashMap<NodeId, Receiver<NetworkMessage<M>>>,
    to_node_senders: HashMap<NodeId, Sender<NetworkMessage<M>>>,
    seed: u64,
}

impl<M> Network<M>
where
    M: std::fmt::Debug + Send + Sync + Clone,
{
    pub fn new(regions: regions::RegionsData, seed: u64) -> Self {
        Self {
            regions,
            network_time: Instant::now(),
            messages: Vec::new(),
            node_network_capacity: HashMap::new(),
            from_node_receivers: HashMap::new(),
            from_node_broadcast_receivers: HashMap::new(),
            to_node_senders: HashMap::new(),
            seed,
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
        capacity_bps: u32,
        node_message_receiver: Receiver<NetworkMessage<M>>,
        node_message_broadcast_receiver: Receiver<NetworkMessage<M>>,
    ) -> Receiver<NetworkMessage<M>> {
        self.node_network_capacity
            .insert(node_id, NodeNetworkCapacity::new(capacity_bps));
        let (to_node_sender, from_network_receiver) = channel::unbounded();
        self.from_node_receivers
            .insert(node_id, node_message_receiver);
        self.from_node_broadcast_receivers
            .insert(node_id, node_message_broadcast_receiver);
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
        let mut adhoc_messages = self
            .from_node_receivers
            .par_iter()
            .flat_map(|(_, from_node)| {
                from_node
                    .try_iter()
                    .map(|msg| (self.network_time, msg))
                    .collect::<Vec<_>>()
            })
            .collect();
        self.messages.append(&mut adhoc_messages);

        let mut broadcast_messages = self
            .from_node_broadcast_receivers
            .iter()
            .flat_map(|(_, from_node)| {
                from_node.try_iter().flat_map(|msg| {
                    self.to_node_senders.keys().map(move |recipient| {
                        let mut m = msg.clone();
                        m.to = Some(*recipient);
                        m
                    })
                })
            })
            .map(|m| (self.network_time, m))
            .collect::<Vec<_>>();
        self.messages.append(&mut broadcast_messages);
    }

    /// Reiterate all messages and send to appropriate nodes if simulated
    /// delay has passed.
    pub fn dispatch_after(&mut self, time_passed: Duration) {
        self.network_time += time_passed;

        let delayed = self
            .messages
            .par_iter()
            .filter(|(network_time, message)| {
                let mut rng = SmallRng::seed_from_u64(self.seed);
                self.send_or_drop_message(&mut rng, network_time, message)
            })
            .cloned()
            .collect();

        for (_, c) in self.node_network_capacity.iter() {
            c.flush_load();
        }

        self.messages = delayed;
    }

    /// Returns true if message needs to be delayed and be dispatched in future.
    fn send_or_drop_message<R: Rng>(
        &self,
        rng: &mut R,
        network_time: &NetworkTime,
        message: &NetworkMessage<M>,
    ) -> bool {
        let to = message.to.expect("adhoc message has recipient");
        if let Some(delay) = self.send_message_cost(rng, message.from, to) {
            let node_capacity = self.node_network_capacity.get(&to).unwrap();
            let should_delay = network_time.add(delay) <= self.network_time;
            let remaining_size = message.remaining_size();
            if should_delay && node_capacity.increase_load(remaining_size) {
                let to_node = self.to_node_senders.get(&to).unwrap();
                to_node
                    .send(message.clone())
                    .expect("node should have connection");
                node_capacity.decrease_load(remaining_size);
                return false;
            } else {
                // if we do not need to delay, then we should check if the msg is too large
                // if so, we mock the partial sending message behavior
                if should_delay {
                    let mut cap = node_capacity.current_load.lock();
                    let sent = node_capacity.capacity_bps - *cap;
                    *cap = node_capacity.capacity_bps;
                    if message.partial_send(sent) == 0 {
                        let to_node = self.to_node_senders.get(&to).unwrap();
                        to_node
                            .send(message.clone())
                            .expect("node should have connection");
                        node_capacity.decrease_load(sent);
                        return false;
                    }
                }
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug)]
pub struct NetworkMessage<M> {
    pub from: NodeId,
    pub to: Option<NodeId>,
    pub payload: M,
    pub remaining: Arc<AtomicU32>,
}

impl<M> NetworkMessage<M> {
    pub fn new(from: NodeId, to: Option<NodeId>, payload: M, size_bytes: u32) -> Self {
        Self {
            from,
            to,
            payload,
            remaining: Arc::new(AtomicU32::new(size_bytes)),
        }
    }

    pub fn payload(&self) -> &M {
        &self.payload
    }

    pub fn into_payload(self) -> M {
        self.payload
    }

    fn remaining_size(&self) -> u32 {
        self.remaining.load(Ordering::SeqCst)
    }

    /// Mock the partial sending of a message behavior, returning the remaining message size.
    fn partial_send(&self, size: u32) -> u32 {
        self.remaining
            .fetch_sub(size, Ordering::SeqCst)
            .saturating_sub(size)
    }
}

pub trait PayloadSize {
    fn size_bytes(&self) -> u32;
}

pub trait NetworkInterface {
    type Payload;

    fn broadcast(&self, message: Self::Payload);
    fn send_message(&self, address: NodeId, message: Self::Payload);
    fn receive_messages(&self) -> Vec<NetworkMessage<Self::Payload>>;
}

pub struct InMemoryNetworkInterface<M> {
    id: NodeId,
    broadcast: Sender<NetworkMessage<M>>,
    sender: Sender<NetworkMessage<M>>,
    receiver: Receiver<NetworkMessage<M>>,
}

impl<M> InMemoryNetworkInterface<M> {
    pub fn new(
        id: NodeId,
        broadcast: Sender<NetworkMessage<M>>,
        sender: Sender<NetworkMessage<M>>,
        receiver: Receiver<NetworkMessage<M>>,
    ) -> Self {
        Self {
            id,
            broadcast,
            sender,
            receiver,
        }
    }
}

impl<M: PayloadSize> NetworkInterface for InMemoryNetworkInterface<M> {
    type Payload = M;

    fn broadcast(&self, message: Self::Payload) {
        let size = message.size_bytes();
        let message = NetworkMessage::new(self.id, None, message, size);
        self.broadcast.send(message).unwrap();
    }

    fn send_message(&self, address: NodeId, message: Self::Payload) {
        let size = message.size_bytes();
        let message = NetworkMessage::new(self.id, Some(address), message, size);
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
        broadcast: Sender<NetworkMessage<()>>,
        sender: Sender<NetworkMessage<()>>,
        receiver: Receiver<NetworkMessage<()>>,
    }

    impl MockNetworkInterface {
        pub fn new(
            id: NodeId,
            broadcast: Sender<NetworkMessage<()>>,
            sender: Sender<NetworkMessage<()>>,
            receiver: Receiver<NetworkMessage<()>>,
        ) -> Self {
            Self {
                id,
                broadcast,
                sender,
                receiver,
            }
        }
    }

    impl NetworkInterface for MockNetworkInterface {
        type Payload = ();

        fn broadcast(&self, message: Self::Payload) {
            let message = NetworkMessage::new(self.id, None, message, 1);
            self.broadcast.send(message).unwrap();
        }

        fn send_message(&self, address: NodeId, message: Self::Payload) {
            let message = NetworkMessage::new(self.id, Some(address), message, 1);
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
        let mut network = Network::new(regions_data, 0);

        let (from_a_sender, from_a_receiver) = channel::unbounded();
        let (from_a_broadcast_sender, from_a_broadcast_receiver) = channel::unbounded();
        let to_a_receiver = network.connect(node_a, 3, from_a_receiver, from_a_broadcast_receiver);
        let a = MockNetworkInterface::new(
            node_a,
            from_a_broadcast_sender,
            from_a_sender,
            to_a_receiver,
        );

        let (from_b_sender, from_b_receiver) = channel::unbounded();
        let (from_b_broadcast_sender, from_b_broadcast_receiver) = channel::unbounded();
        let to_b_receiver = network.connect(node_b, 3, from_b_receiver, from_b_broadcast_receiver);
        let b = MockNetworkInterface::new(
            node_b,
            from_b_broadcast_sender,
            from_b_sender,
            to_b_receiver,
        );

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
        let mut network = Network::new(regions_data, 0);

        let (from_a_sender, from_a_receiver) = channel::unbounded();
        let (from_a_broadcast_sender, from_a_broadcast_receiver) = channel::unbounded();
        let to_a_receiver = network.connect(node_a, 2, from_a_receiver, from_a_broadcast_receiver);
        let a = MockNetworkInterface::new(
            node_a,
            from_a_broadcast_sender,
            from_a_sender,
            to_a_receiver,
        );

        let (from_b_sender, from_b_receiver) = channel::unbounded();
        let (from_b_broadcast_sender, from_b_broadcast_receiver) = channel::unbounded();
        let to_b_receiver = network.connect(node_b, 2, from_b_receiver, from_b_broadcast_receiver);
        let b = MockNetworkInterface::new(
            node_b,
            from_b_broadcast_sender,
            from_b_sender,
            to_b_receiver,
        );

        let (from_c_sender, from_c_receiver) = channel::unbounded();
        let (from_c_broadcast_sender, from_c_broadcast_receiver) = channel::unbounded();
        let to_c_receiver = network.connect(node_c, 2, from_c_receiver, from_c_broadcast_receiver);
        let c = MockNetworkInterface::new(
            node_c,
            from_c_broadcast_sender,
            from_c_sender,
            to_c_receiver,
        );

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

    #[test]
    fn node_network_capacity_limit() {
        let node_a = NodeId::from_index(0);
        let node_b = NodeId::from_index(1);

        let regions = HashMap::from([(Region::Europe, vec![node_a, node_b])]);
        let behaviour = HashMap::from([(
            NetworkBehaviourKey::new(Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        )]);
        let regions_data = RegionsData::new(regions, behaviour);
        let mut network = Network::new(regions_data, 0);

        let (from_a_sender, from_a_receiver) = channel::unbounded();
        let (from_a_broadcast_sender, from_a_broadcast_receiver) = channel::unbounded();
        let to_a_receiver = network.connect(node_a, 3, from_a_receiver, from_a_broadcast_receiver);
        let a = MockNetworkInterface::new(
            node_a,
            from_a_broadcast_sender,
            from_a_sender,
            to_a_receiver,
        );

        let (from_b_sender, from_b_receiver) = channel::unbounded();
        let (from_b_broadcast_sender, from_b_broadcast_receiver) = channel::unbounded();
        let to_b_receiver = network.connect(node_b, 2, from_b_receiver, from_b_broadcast_receiver);
        let b = MockNetworkInterface::new(
            node_b,
            from_b_broadcast_sender,
            from_b_sender,
            to_b_receiver,
        );

        for _ in 0..6 {
            a.send_message(node_b, ());
            b.send_message(node_a, ());
        }

        network.step(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 3);
        assert_eq!(b.receive_messages().len(), 2);

        network.step(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 3);
        assert_eq!(b.receive_messages().len(), 2);

        network.step(Duration::from_millis(100));
        assert_eq!(a.receive_messages().len(), 0);
        assert_eq!(b.receive_messages().len(), 2);
    }
}

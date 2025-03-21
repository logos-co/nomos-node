mod config;

use std::{
    error::Error,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use blake2::{
    digest::{consts::U32, Digest},
    Blake2b,
};
pub use config::{secret_key_serde, SwarmConfig};
pub use libp2p::{
    self,
    core::upgrade,
    gossipsub::{self, PublishError, SubscriptionError},
    identity::{self, ed25519},
    swarm::{dial_opts::DialOpts, DialError, NetworkBehaviour, SwarmEvent},
    PeerId, SwarmBuilder, Transport,
};
use libp2p::{
    gossipsub::{Message, MessageId, TopicHash},
    identify,
    kad::{self, QueryId},
    swarm::ConnectionId,
    StreamProtocol,
};
pub use multiaddr::{multiaddr, Multiaddr, Protocol};

// The protocol name for the Kademlia protocol
// This makes all node use the same protocol name
// todo: figure out if we need to handle multiple networks that should not be
// connected (states)
pub const KAD_PROTOCOL_NAME: &str = "/nomos/kad/1.0.0";

// TODO: Risc0 proofs are HUGE (220 Kb) and it's the only reason we need to have
// this limit so large. Remove this once we transition to smaller proofs.
const DATA_LIMIT: usize = 1 << 18; // Do not serialize/deserialize more than 256 KiB

/// Wraps [`libp2p::Swarm`], and config it for use within Nomos.
pub struct Swarm {
    // A core libp2p swarm
    swarm: libp2p::Swarm<Behaviour>,
}

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<kad::store::MemoryStore>, // todo: support persistent store if needed
    identify: identify::Behaviour,
}

impl Behaviour {
    fn new(
        peer_id: PeerId,
        gossipsub_config: gossipsub::Config,
        publick_key: identity::PublicKey,
    ) -> Result<Self, Box<dyn Error>> {
        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Author(peer_id),
            gossipsub::ConfigBuilder::from(gossipsub_config)
                .validation_mode(gossipsub::ValidationMode::None)
                .message_id_fn(compute_message_id)
                .max_transmit_size(DATA_LIMIT)
                .build()?,
        )?;

        // make config parameters configurable
        let store = kad::store::MemoryStore::new(peer_id);
        let mut kad_config = kad::Config::new(StreamProtocol::new(KAD_PROTOCOL_NAME));
        kad_config.set_periodic_bootstrap_interval(Some(Duration::from_secs(10))); // this interval is only for proof of concept and should be configurable
        let kademlia = kad::Behaviour::with_config(peer_id, store, kad_config);

        let identify = identify::Behaviour::new(identify::Config::new(
            "/nomos/ide/1.0.0".into(), // Identify protocol name
            publick_key,
        ));

        Ok(Self {
            gossipsub,
            kademlia,
            identify,
        })
    }

    pub fn kademlia_add_address(&mut self, peer_id: PeerId, addr: Multiaddr) {
        self.kademlia.add_address(&peer_id, addr);
    }

    pub fn kademlia_routing_table_dump(&mut self) -> Vec<PeerId> {
        self.kademlia
            .kbuckets()
            .flat_map(|bucket| {
                let peers = bucket
                    .iter()
                    .map(|entry| *entry.node.key.preimage())
                    .collect::<Vec<_>>();

                peers
            })
            .collect()
    }

    pub fn bootstrap_kademlia(&mut self) {
        let res = self.kademlia.bootstrap();
        if let Err(e) = res {
            tracing::error!("failed to bootstrap kademlia: {e}");
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SwarmError {
    #[error("duplicate dialing")]
    DuplicateDialing,

    #[error("no known peers")]
    NoKnownPeers,
}

/// How long to keep a connection alive once it is idling.
const IDLE_CONN_TIMEOUT: Duration = Duration::from_secs(300);

impl Swarm {
    /// Builds a [`Swarm`] configured for use with Nomos on top of a tokio
    /// executor.
    //
    // TODO: define error types
    pub fn build(config: &SwarmConfig) -> Result<Self, Box<dyn Error>> {
        let keypair =
            libp2p::identity::Keypair::from(ed25519::Keypair::from(config.node_key.clone()));
        let peer_id = PeerId::from(keypair.public());
        tracing::info!("libp2p peer_id:{}", peer_id);

        let mut swarm = libp2p::SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_quic()
            .with_dns()?
            .with_behaviour(|keypair| {
                Behaviour::new(peer_id, config.gossipsub_config.clone(), keypair.public()).unwrap()
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(IDLE_CONN_TIMEOUT))
            .build();

        let listen_addr = Self::multiaddr(config.host, config.port);
        swarm.listen_on(listen_addr.clone())?;

        // Add our own listening address as external to enable server mode for Kademlia
        // todo: figure out if we need to also support client mode and make it
        // configurable
        let external_addr = listen_addr.with(Protocol::P2p(peer_id));
        swarm.add_external_address(external_addr.clone());
        tracing::info!("Added external address: {}", external_addr);

        Ok(Self { swarm })
    }

    /// Initiates a connection attempt to a peer
    pub fn connect(&mut self, peer_addr: &Multiaddr) -> Result<ConnectionId, DialError> {
        let opt = DialOpts::from(peer_addr.clone());
        let connection_id = opt.connection_id();

        tracing::debug!("attempting to dial {peer_addr}. connection_id:{connection_id:?}",);
        self.swarm.dial(opt)?;
        Ok(connection_id)
    }

    /// Subscribes to a topic
    ///
    /// Returns true if the topic is newly subscribed or false if already
    /// subscribed.
    pub fn subscribe(&mut self, topic: &str) -> Result<bool, SubscriptionError> {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&gossipsub::IdentTopic::new(topic))
    }

    pub fn broadcast(
        &mut self,
        topic: &str,
        message: impl Into<Vec<u8>>,
    ) -> Result<MessageId, PublishError> {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(gossipsub::IdentTopic::new(topic), message)
    }

    /// Unsubscribes from a topic
    ///
    /// Returns true if previously subscribed
    pub fn unsubscribe(&mut self, topic: &str) -> bool {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .unsubscribe(&gossipsub::IdentTopic::new(topic))
    }

    /// Returns a reference to the underlying [`libp2p::Swarm`]
    pub const fn swarm(&self) -> &libp2p::Swarm<Behaviour> {
        &self.swarm
    }

    pub const fn swarm_mut(&mut self) -> &mut libp2p::Swarm<Behaviour> {
        &mut self.swarm
    }

    pub fn is_subscribed(&mut self, topic: &str) -> bool {
        let topic_hash = Self::topic_hash(topic);

        //TODO: consider O(1) searching by having our own data structure
        self.swarm
            .behaviour_mut()
            .gossipsub
            .topics()
            .any(|h| h == &topic_hash)
    }

    pub fn get_closest_peers(&mut self, peer_id: libp2p::PeerId) -> QueryId {
        self.swarm
            .behaviour_mut()
            .kademlia
            .get_closest_peers(peer_id)
    }

    #[must_use]
    pub fn topic_hash(topic: &str) -> TopicHash {
        gossipsub::IdentTopic::new(topic).hash()
    }

    #[must_use]
    pub fn multiaddr(ip: std::net::Ipv4Addr, port: u16) -> Multiaddr {
        multiaddr!(Ip4(ip), Udp(port), QuicV1)
    }
}

impl futures::Stream for Swarm {
    type Item = SwarmEvent<BehaviourEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.swarm).poll_next(cx)
    }
}

fn compute_message_id(message: &Message) -> MessageId {
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(&message.data);
    MessageId::from(hasher.finalize().to_vec())
}

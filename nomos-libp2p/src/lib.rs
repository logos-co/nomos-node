mod config;

use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use std::time::Duration;

pub use config::SwarmConfig;
pub use libp2p;

use libp2p::gossipsub::{MessageId, TopicHash};
#[allow(deprecated)]
pub use libp2p::{
    core::upgrade,
    dns,
    gossipsub::{self, PublishError, SubscriptionError},
    identity::{self, secp256k1},
    plaintext::Config as PlainText2Config,
    swarm::{dial_opts::DialOpts, DialError, NetworkBehaviour, SwarmEvent, THandlerErr},
    tcp, yamux, PeerId, SwarmBuilder, Transport,
};
use libp2p::{swarm::ConnectionId, tcp::tokio::Tcp};
pub use multiaddr::{multiaddr, Multiaddr, Protocol};

/// Wraps [`libp2p::Swarm`], and config it for use within Nomos.
pub struct Swarm {
    // A core libp2p swarm
    swarm: libp2p::Swarm<Behaviour>,
}

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    gossipsub: gossipsub::Behaviour,
}

#[derive(thiserror::Error, Debug)]
pub enum SwarmError {
    #[error("duplicate dialing")]
    DuplicateDialing,
}

/// A timeout for the setup and protocol upgrade process for all in/outbound connections
const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(20);

impl Swarm {
    /// Builds a [`Swarm`] configured for use with Nomos on top of a tokio executor.
    //
    // TODO: define error types
    pub fn build(config: &SwarmConfig) -> Result<Self, Box<dyn Error>> {
        let id_keys = identity::Keypair::from(secp256k1::Keypair::from(config.node_key.clone()));
        let local_peer_id = PeerId::from(id_keys.public());
        log::info!("libp2p peer_id:{}", local_peer_id);

        // TODO: consider using noise authentication
        let tcp_transport = tcp::Transport::<Tcp>::new(tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1Lazy)
            .authenticate(PlainText2Config::new(&id_keys))
            .multiplex(yamux::Config::default())
            .timeout(TRANSPORT_TIMEOUT)
            .boxed();

        // Wrapping TCP transport into DNS transport to resolve hostnames.
        let tcp_transport = dns::tokio::Transport::system(tcp_transport)?.boxed();

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Author(local_peer_id),
            gossipsub::ConfigBuilder::from(config.gossipsub_config.clone())
                // To use the default `message_id_fn` that uses the message sender info,
                // we shouldn't use ValidationMode::None that discards sender info
                // from the received message, which causes the message ID to be created wrongly.
                // https://github.com/libp2p/rust-libp2p/blob/ec157171a7c1d733dcaff80b00ad9596c15701a1/protocols/gossipsub/src/protocol.rs#L377
                .validation_mode(gossipsub::ValidationMode::Permissive)
                .build()?,
        )?;

        let mut swarm = libp2p::Swarm::new(
            tcp_transport,
            Behaviour { gossipsub },
            local_peer_id,
            libp2p::swarm::Config::with_tokio_executor(),
        );

        swarm.listen_on(multiaddr!(Ip4(config.host), Tcp(config.port)))?;

        Ok(Swarm { swarm })
    }

    /// Initiates a connection attempt to a peer
    pub fn connect(&mut self, peer_addr: Multiaddr) -> Result<ConnectionId, DialError> {
        let opt = DialOpts::from(peer_addr.clone());
        let connection_id = opt.connection_id();

        tracing::debug!("attempting to dial {peer_addr}. connection_id:{connection_id:?}",);
        self.swarm.dial(opt)?;
        Ok(connection_id)
    }

    /// Subscribes to a topic
    ///
    /// Returns true if the topic is newly subscribed or false if already subscribed.
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
    pub fn unsubscribe(&mut self, topic: &str) -> Result<bool, PublishError> {
        self.swarm
            .behaviour_mut()
            .gossipsub
            .unsubscribe(&gossipsub::IdentTopic::new(topic))
    }

    /// Returns a reference to the underlying [`libp2p::Swarm`]
    pub fn swarm(&self) -> &libp2p::Swarm<Behaviour> {
        &self.swarm
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

    pub fn topic_hash(topic: &str) -> TopicHash {
        gossipsub::IdentTopic::new(topic).hash()
    }
}

impl futures::Stream for Swarm {
    #[allow(deprecated)]
    type Item = SwarmEvent<BehaviourEvent, THandlerErr<Behaviour>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.swarm).poll_next(cx)
    }
}

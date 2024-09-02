mod config;

use std::error::Error;
use std::pin::Pin;
use std::task::{Context, Poll};

use std::time::Duration;

pub use config::{secret_key_serde, SwarmConfig};
pub use libp2p;

use blake2::digest::{consts::U32, Digest};
use blake2::Blake2b;
use libp2p::gossipsub::{Message, MessageId, TopicHash};
use libp2p::swarm::ConnectionId;
pub use libp2p::{
    core::upgrade,
    gossipsub::{self, PublishError, SubscriptionError},
    identity::{self, ed25519},
    swarm::{dial_opts::DialOpts, DialError, NetworkBehaviour, SwarmEvent},
    PeerId, SwarmBuilder, Transport,
};
pub use libp2p_stream;
use libp2p_stream::Control;
pub use multiaddr::{multiaddr, Multiaddr, Protocol};

/// Wraps [`libp2p::Swarm`], and config it for use within Nomos.
pub struct Swarm {
    // A core libp2p swarm
    swarm: libp2p::Swarm<Behaviour>,
}

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    stream: libp2p_stream::Behaviour,
    gossipsub: gossipsub::Behaviour,
}

impl Behaviour {
    fn new(peer_id: PeerId, gossipsub_config: gossipsub::Config) -> Result<Self, Box<dyn Error>> {
        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Author(peer_id),
            gossipsub::ConfigBuilder::from(gossipsub_config)
                .validation_mode(gossipsub::ValidationMode::None)
                .message_id_fn(compute_message_id)
                .max_transmit_size(1 << 19)
                .build()?,
        )?;
        Ok(Self {
            stream: libp2p_stream::Behaviour::new(),
            gossipsub,
        })
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SwarmError {
    #[error("duplicate dialing")]
    DuplicateDialing,
}

/// How long to keep a connection alive once it is idling.
const IDLE_CONN_TIMEOUT: Duration = Duration::from_secs(300);

impl Swarm {
    /// Builds a [`Swarm`] configured for use with Nomos on top of a tokio executor.
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
            .with_behaviour(|_| Behaviour::new(peer_id, config.gossipsub_config.clone()).unwrap())?
            .with_swarm_config(|c| c.with_idle_connection_timeout(IDLE_CONN_TIMEOUT))
            .build();

        swarm.listen_on(Self::multiaddr(config.host, config.port))?;

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

    /// Returns a stream control that can be used to accept streams and establish streams to
    /// other peers.
    /// Stream controls can be cloned.
    pub fn stream_control(&self) -> Control {
        self.swarm.behaviour().stream.new_control()
    }

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
    use libp2p::StreamProtocol;
    use rand::Rng;

    use crate::{Swarm, SwarmConfig};

    #[tokio::test]
    async fn stream() {
        // Init two swarms
        let (config1, mut swarm1) = init_swarm();
        let (_, mut swarm2) = init_swarm();
        let swarm1_peer_id = *swarm1.swarm().local_peer_id();

        // Dial to swarm1
        swarm2
            .connect(Swarm::multiaddr(config1.host, config1.port))
            .unwrap();

        // Prepare stream controls
        let mut stream_control1 = swarm1.stream_control();
        let mut stream_control2 = swarm2.stream_control();

        // Poll swarms to make progress
        tokio::spawn(async move { while (swarm1.next().await).is_some() {} });
        tokio::spawn(async move { while (swarm2.next().await).is_some() {} });

        // Make swarm1 accept incoming streams
        let protocol = StreamProtocol::new("/test");
        let mut incoming_streams = stream_control1.accept(protocol).unwrap();
        tokio::spawn(async move {
            // If a new stream is established, write bytes and close the stream.
            while let Some((_, mut stream)) = incoming_streams.next().await {
                stream.write_all(&[1, 2, 3, 4]).await.unwrap();
                stream.close().await.unwrap();
            }
        });

        // Wait until the connection is established
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Establish a stream with swarm1 and read bytes
        let mut stream = stream_control2
            .open_stream(swarm1_peer_id, StreamProtocol::new("/test"))
            .await
            .unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    fn init_swarm() -> (SwarmConfig, Swarm) {
        let config = SwarmConfig {
            host: std::net::Ipv4Addr::new(127, 0, 0, 1),
            port: rand::thread_rng().gen_range(10000..30000),
            ..Default::default()
        };
        let swarm = Swarm::build(&config).unwrap();
        (config, swarm)
    }
}

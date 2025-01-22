mod behaviour;
mod error;
mod handler;

#[cfg(feature = "tokio")]
use std::time::Duration;

pub use behaviour::{Behaviour, Config, Event, IntervalStreamProvider};

#[cfg(feature = "tokio")]
pub struct TokioIntervalStreamProvider;

#[cfg(feature = "tokio")]
impl IntervalStreamProvider for TokioIntervalStreamProvider {
    type Stream = tokio_stream::wrappers::IntervalStream;

    fn interval_stream(interval: Duration) -> Self::Stream {
        // Since tokio::time::interval.tick() returns immediately regardless of the interval,
        // we need to explicitly specify the time of the first tick we expect.
        // If not, the peer would be marked as unhealthy immediately
        // as soon as the connection is established.
        let start = tokio::time::Instant::now() + interval;
        tokio_stream::wrappers::IntervalStream::new(tokio::time::interval_at(start, interval))
    }
}

#[cfg(test)]
#[cfg(feature = "tokio")]
mod test {
    use std::time::Duration;

    use libp2p::{
        futures::StreamExt,
        identity::Keypair,
        swarm::{dummy, NetworkBehaviour, SwarmEvent},
        Multiaddr, PeerId, Swarm, SwarmBuilder,
    };
    use nomos_blend::{conn_maintenance::ConnectionMonitorSettings, membership::Node};
    use nomos_blend_message::{mock::MockBlendMessage, BlendMessage};
    use tokio::select;

    use crate::{behaviour::Config, error::Error, Behaviour, Event, TokioIntervalStreamProvider};

    /// Check that a published messsage arrives in the peers successfully.
    #[tokio::test]
    async fn behaviour() {
        // Initialize two swarms that support the blend protocol.
        let (mut nodes, mut keypairs) = nodes(2, 8090);
        let node1_addr = nodes.next().unwrap().address;
        let mut swarm1 = new_blend_swarm(keypairs.next().unwrap(), node1_addr.clone(), None);
        let mut swarm2 = new_blend_swarm(
            keypairs.next().unwrap(),
            nodes.next().unwrap().address,
            None,
        );
        swarm2.dial(node1_addr).unwrap();

        // Swamr2 publishes a message.
        let task = async {
            let msg = vec![1; 10];
            let mut msg_published = false;
            let mut publish_try_interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                select! {
                    // Try to publish a message until it succeeds.
                    // (It will fail until swarm2 is connected to swarm1 successfully.)
                    _ = publish_try_interval.tick() => {
                        if !msg_published {
                            msg_published = swarm2.behaviour_mut().publish(msg.clone()).is_ok();
                        }
                    }
                    // Proceed swarm1
                    event = swarm1.select_next_some() => {
                        if let SwarmEvent::Behaviour(Event::Message(received_msg)) = event {
                            assert_eq!(received_msg, msg);
                            break;
                        };
                    }
                    // Proceed swarm2
                    _ = swarm2.select_next_some() => {}
                }
            }
        };

        // Expect for the task to be completed within 30 seconds.
        assert!(tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .is_ok());
    }

    /// If the peer doesn't support the blend protocol, the message should not be forwarded to the peer.
    #[tokio::test]
    async fn peer_not_support_blend_protocol() {
        // Only swarm2 supports the blend protocol.
        let (mut nodes, mut keypairs) = nodes(2, 8190);
        let node1_addr = nodes.next().unwrap().address;
        let mut swarm1 = new_dummy_swarm(keypairs.next().unwrap(), node1_addr.clone());
        let mut swarm2 = new_blend_swarm(
            keypairs.next().unwrap(),
            nodes.next().unwrap().address,
            None,
        );
        swarm2.dial(node1_addr).unwrap();

        // Expect all publish attempts to fail with [`Error::NoPeers`]
        // because swarm2 doesn't have any peers that support the blend protocol.
        let msg = vec![1; 10];
        let mut publish_try_interval = tokio::time::interval(Duration::from_secs(1));
        let mut publish_try_count = 0;
        loop {
            select! {
                _ = publish_try_interval.tick() => {
                    assert!(matches!(swarm2.behaviour_mut().publish(msg.clone()), Err(Error::NoPeers)));
                    publish_try_count += 1;
                    if publish_try_count >= 10 {
                        break;
                    }
                }
                _ = swarm1.select_next_some() => {}
                _ = swarm2.select_next_some() => {}
            }
        }
    }

    fn new_blend_swarm(
        keypair: Keypair,
        addr: Multiaddr,
        conn_monitor_settings: Option<ConnectionMonitorSettings>,
    ) -> Swarm<Behaviour<MockBlendMessage, TokioIntervalStreamProvider>> {
        new_swarm_with_behaviour(
            keypair,
            addr,
            Behaviour::<MockBlendMessage, TokioIntervalStreamProvider>::new(Config {
                duplicate_cache_lifespan: 60,
                conn_monitor_settings,
            }),
        )
    }

    fn new_dummy_swarm(keypair: Keypair, addr: Multiaddr) -> Swarm<dummy::Behaviour> {
        new_swarm_with_behaviour(keypair, addr, dummy::Behaviour)
    }

    fn new_swarm_with_behaviour<B: NetworkBehaviour>(
        keypair: Keypair,
        addr: Multiaddr,
        behaviour: B,
    ) -> Swarm<B> {
        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_other_transport(|keypair| {
                libp2p::quic::tokio::Transport::new(libp2p::quic::Config::new(keypair))
            })
            .unwrap()
            .with_behaviour(|_| behaviour)
            .unwrap()
            .build();
        swarm.listen_on(addr).unwrap();
        swarm
    }

    fn nodes(
        count: usize,
        base_port: usize,
    ) -> (
        impl Iterator<Item = Node<PeerId, <MockBlendMessage as BlendMessage>::PublicKey>>,
        impl Iterator<Item = Keypair>,
    ) {
        let mut nodes = Vec::with_capacity(count);
        let mut keypairs = Vec::with_capacity(count);

        for i in 0..count {
            let keypair = Keypair::generate_ed25519();
            let node = Node {
                id: PeerId::from(keypair.public()),
                address: format!("/ip4/127.0.0.1/udp/{}/quic-v1", base_port + i)
                    .parse()
                    .unwrap(),
                public_key: [i as u8; 32],
            };
            nodes.push(node);
            keypairs.push(keypair);
        }

        (nodes.into_iter(), keypairs.into_iter())
    }
}

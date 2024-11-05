mod behaviour;
mod error;
mod handler;

pub use behaviour::{Behaviour, Config, Event};

#[cfg(test)]
mod test {
    use std::time::Duration;

    use libp2p::{
        futures::StreamExt,
        identity::Keypair,
        swarm::{dummy, NetworkBehaviour, SwarmEvent},
        Multiaddr, PeerId, Swarm, SwarmBuilder,
    };
    use nomos_mix_message::mock::MockMixMessage;
    use tokio::select;

    use crate::{behaviour::Config, error::Error, Behaviour, Event};

    /// Check that a published messsage arrives in the peers successfully.
    #[tokio::test]
    async fn behaviour() {
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let peer_id1 = PeerId::from_public_key(&k1.public());
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        // Initialize two swarms that support the mix protocol.
        let mut swarm1 = new_swarm(k1);
        let mut swarm2 = new_swarm(k2);

        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5073/quic-v1".parse().unwrap();
        let addr_with_peer_id = addr.clone().with_p2p(peer_id1).unwrap();

        // Swarm1 listens on the address.
        swarm1.listen_on(addr).unwrap();

        // Dial to swarm1 from swarm2
        tokio::time::sleep(Duration::from_secs(1)).await;
        swarm2.dial(addr_with_peer_id).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

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

    /// If the peer doesn't support the mix protocol, the message should not be forwarded to the peer.
    #[tokio::test]
    async fn peer_not_support_mix_protocol() {
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let peer_id1 = PeerId::from_public_key(&k1.public());
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        // Only swarm2 supports the mix protocol.
        let mut swarm1 = new_swarm_without_mix(k1);
        let mut swarm2 = new_swarm(k2);

        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5074/quic-v1".parse().unwrap();
        let addr_with_peer_id = addr.clone().with_p2p(peer_id1).unwrap();

        // Swarm1 listens on the address.
        swarm1.listen_on(addr).unwrap();

        // Dial to swarm1 from swarm2
        tokio::time::sleep(Duration::from_secs(1)).await;
        swarm2.dial(addr_with_peer_id).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Expect all publish attempts to fail with [`Error::NoPeers`]
        // because swarm2 doesn't have any peers that support the mix protocol.
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

    fn new_swarm(key: Keypair) -> Swarm<Behaviour<MockMixMessage>> {
        new_swarm_with_behaviour(
            key,
            Behaviour::new(Config {
                duplicate_cache_lifespan: 60,
            }),
        )
    }

    fn new_swarm_without_mix(key: Keypair) -> Swarm<dummy::Behaviour> {
        new_swarm_with_behaviour(key, dummy::Behaviour)
    }

    fn new_swarm_with_behaviour<B: NetworkBehaviour>(key: Keypair, behaviour: B) -> Swarm<B> {
        SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|keypair| {
                libp2p::quic::tokio::Transport::new(libp2p::quic::Config::new(keypair))
            })
            .unwrap()
            .with_behaviour(|_| behaviour)
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
            })
            .build()
    }
}

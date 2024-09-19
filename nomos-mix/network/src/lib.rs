mod behaviour;
mod error;
mod handler;

pub use behaviour::{Behaviour, Event};

#[cfg(test)]
mod test {
    use std::time::Duration;

    use libp2p::{
        futures::StreamExt,
        identity::Keypair,
        swarm::{dummy, NetworkBehaviour, SwarmEvent},
        Multiaddr, PeerId, Swarm, SwarmBuilder,
    };
    use nomos_mix_message::{new_message, MSG_SIZE};
    use tokio::select;

    use crate::{error::Error, Behaviour, Event};

    /// Check that an wrapped message is forwarded through mix nodes and unwrapped successfully.
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

        // Spawn swarm1
        tokio::spawn(async move {
            swarm1.listen_on(addr).unwrap();
            loop {
                swarm1.select_next_some().await;
            }
        });

        // Dial to swarm1 from swarm2
        tokio::time::sleep(Duration::from_secs(1)).await;
        swarm2.dial(addr_with_peer_id).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Prepare a task for swarm2 to publish a two-layer wrapped message,
        // receive an one-layer unwrapped message from swarm1,
        // and return a fully unwrapped message.
        let task = async {
            let mut msg_published = false;
            let mut publish_try_interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                select! {
                    // Try to publish a message until it succeeds.
                    // (It will fail until swarm2 is connected to swarm1 successfully.)
                    _ = publish_try_interval.tick() => {
                        if !msg_published {
                            // Prepare a message wrapped in two layers
                            let msg = new_message(&[1; MSG_SIZE - 1], 2).unwrap();
                            msg_published = swarm2.behaviour_mut().publish(msg).is_ok();
                        }
                    }
                    // Proceed swarm2
                    event = swarm2.select_next_some() => {
                        if let SwarmEvent::Behaviour(Event::FullyUnwrappedMessage(message)) = event {
                            println!("SWARM2 FULLY_UNWRAPPED_MESSAGE: {:?}", message);
                            break;
                        };
                    }
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

        // Spawn swarm1
        tokio::spawn(async move {
            swarm1.listen_on(addr).unwrap();
            loop {
                swarm1.select_next_some().await;
            }
        });

        // Dial to swarm1 from swarm2
        tokio::time::sleep(Duration::from_secs(1)).await;
        swarm2.dial(addr_with_peer_id).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Expect all publish attempts to fail with [`Error::NoPeers`]
        // because swarm2 doesn't have any peers that support the mix protocol.
        let mut publish_try_interval = tokio::time::interval(Duration::from_secs(1));
        let mut publish_try_count = 0;
        loop {
            select! {
                _ = publish_try_interval.tick() => {
                    let msg = new_message(&[10; MSG_SIZE - 1], 1).unwrap();
                    assert!(matches!(swarm2.behaviour_mut().publish(msg), Err(Error::NoPeers)));
                    publish_try_count += 1;
                    if publish_try_count >= 10 {
                        break;
                    }
                }
                _ = swarm2.select_next_some() => {}
            }
        }
    }

    fn new_swarm(key: Keypair) -> Swarm<Behaviour> {
        new_swarm_with_behaviour(key, Behaviour::new())
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

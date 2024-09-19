mod behaviour;
mod error;
mod handler;

#[cfg(test)]
mod test {
    use std::time::Duration;

    use libp2p::{
        futures::StreamExt,
        gossipsub,
        identity::Keypair,
        swarm::{NetworkBehaviour, SwarmEvent},
        Multiaddr, PeerId, Swarm, SwarmBuilder,
    };
    use nomos_mix_message::{new_message, MSG_SIZE};
    use tokio::select;

    use crate::{
        behaviour::{Behaviour, Event, MixBehaviour},
        error::Error,
    };

    #[tokio::test]
    async fn behaviour() {
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let peer_id1 = PeerId::from_public_key(&k1.public());
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        let mut swarm1 = new_swarm(k1);
        let mut swarm2 = new_swarm(k2);

        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5073/quic-v1".parse().unwrap();
        let addr_with_peer_id = addr.clone().with_p2p(peer_id1).unwrap();

        tokio::spawn(async move {
            swarm1.listen_on(addr).unwrap();
            loop {
                if let SwarmEvent::Behaviour(Event::Gossipsub(gossipsub::Event::Message {
                    message,
                    ..
                })) = swarm1.select_next_some().await
                {
                    println!("SWARM1 GOSSIPPED_MSG: {:?}", message.data);
                }
            }
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        swarm2.dial(addr_with_peer_id).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

        let task = async {
            let mut msg_published = false;
            let mut publish_try_interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                select! {
                    _ = publish_try_interval.tick() => {
                        if !msg_published {
                            let msg = new_message(&[10; MSG_SIZE - 1], 1).unwrap();
                            msg_published = swarm2.behaviour_mut().publish(msg).is_ok();
                        }
                    }
                    event = swarm2.select_next_some() => {
                        if let SwarmEvent::Behaviour(Event::Gossipsub(gossipsub::Event::Message {
                                message,
                                ..
                            })) = event {
                            println!("SWARM2 GOSSIPPED_MSG: {:?}", message.data);
                            break;
                        };
                    }
                }
            }
        };

        assert!(tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn unsupported() {
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let peer_id1 = PeerId::from_public_key(&k1.public());
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        let mut swarm1 = new_swarm_without_mix(k1);
        let mut swarm2 = new_swarm(k2);

        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5074/quic-v1".parse().unwrap();
        let addr_with_peer_id = addr.clone().with_p2p(peer_id1).unwrap();

        tokio::spawn(async move {
            swarm1.listen_on(addr).unwrap();
            loop {
                swarm1.select_next_some().await;
            }
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        swarm2.dial(addr_with_peer_id).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

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
        new_swarm_with_behaviour(
            key,
            Behaviour::new(MixBehaviour::new(), new_gossipsub_behaviour()).unwrap(),
        )
    }

    fn new_swarm_without_mix(key: Keypair) -> Swarm<gossipsub::Behaviour> {
        new_swarm_with_behaviour(key, new_gossipsub_behaviour())
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

    fn new_gossipsub_behaviour() -> gossipsub::Behaviour {
        gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Anonymous,
            gossipsub::ConfigBuilder::default()
                .validation_mode(gossipsub::ValidationMode::None)
                .heartbeat_interval(Duration::from_secs(60))
                .build()
                .unwrap(),
        )
        .unwrap()
    }
}

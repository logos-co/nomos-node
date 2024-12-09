mod behaviour;
mod error;
mod handler;

pub use behaviour::{Behaviour, Config, Event};

#[cfg(test)]
mod test {
    use std::time::Duration;

    use fixed::types::U57F7;
    use libp2p::{
        futures::StreamExt,
        swarm::{dummy, NetworkBehaviour, SwarmEvent},
        Multiaddr, Swarm, SwarmBuilder,
    };
    use nomos_blend::{
        conn_maintenance::{ConnectionMaintenanceSettings, ConnectionMonitorSettings},
        membership::{Membership, Node},
    };
    use nomos_blend_message::{mock::MockBlendMessage, BlendMessage};
    use rand::{rngs::ThreadRng, thread_rng};
    use tokio::select;
    use tokio_stream::wrappers::IntervalStream;

    use crate::{behaviour::Config, error::Error, Behaviour, Event};

    /// Check that a published messsage arrives in the peers successfully.
    #[tokio::test]
    async fn behaviour() {
        // Initialize two swarms that support the blend protocol.
        let nodes = nodes(2, 8090);
        let mut swarm1 = new_swarm(Membership::new(nodes.clone(), nodes[0].public_key));
        let mut swarm2 = new_swarm(Membership::new(nodes.clone(), nodes[1].public_key));

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
        let nodes = nodes(2, 8190);
        let mut swarm1 = new_swarm_without_blend(nodes[0].address.clone());
        let mut swarm2 = new_swarm(Membership::new(nodes.clone(), nodes[1].public_key));

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

    fn new_swarm(
        membership: Membership<MockBlendMessage>,
    ) -> Swarm<Behaviour<MockBlendMessage, ThreadRng, IntervalStream>> {
        let conn_maintenance_settings = ConnectionMaintenanceSettings {
            peering_degree: membership.size() - 1, // excluding the local node
            max_peering_degree: membership.size() * 2,
            monitor: Some(ConnectionMonitorSettings {
                time_window: Duration::from_secs(60),
                expected_effective_messages: U57F7::from_num(1.0),
                effective_message_tolerance: U57F7::from_num(0.1),
                expected_drop_messages: U57F7::from_num(1.0),
                drop_message_tolerance: U57F7::from_num(0.1),
            }),
        };
        let conn_maintenance_interval = conn_maintenance_settings
            .monitor
            .as_ref()
            .map(|monitor| IntervalStream::new(tokio::time::interval(monitor.time_window)));
        let mut swarm = new_swarm_with_behaviour(Behaviour::new(
            Config {
                duplicate_cache_lifespan: 60,
                conn_maintenance_settings,
                conn_maintenance_interval,
            },
            membership.clone(),
            thread_rng(),
        ));
        swarm
            .listen_on(membership.local_node().address.clone())
            .unwrap();
        swarm
    }

    fn new_swarm_without_blend(addr: Multiaddr) -> Swarm<dummy::Behaviour> {
        let mut swarm = new_swarm_with_behaviour(dummy::Behaviour);
        swarm.listen_on(addr).unwrap();
        swarm
    }

    fn new_swarm_with_behaviour<B: NetworkBehaviour>(behaviour: B) -> Swarm<B> {
        SwarmBuilder::with_new_identity()
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

    fn nodes(
        count: usize,
        base_port: usize,
    ) -> Vec<Node<<MockBlendMessage as BlendMessage>::PublicKey>> {
        (0..count)
            .map(|i| Node {
                address: format!("/ip4/127.0.0.1/udp/{}/quic-v1", base_port + i)
                    .parse()
                    .unwrap(),
                public_key: [i as u8; 32],
            })
            .collect()
    }
}

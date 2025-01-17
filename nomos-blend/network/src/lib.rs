mod behaviour;
mod error;
mod handler;

pub use behaviour::{Behaviour, Config, Event, IntervalStreamProvider};

#[cfg(test)]
mod test {
    use std::time::Duration;

    use fixed::types::U57F7;
    use libp2p::{
        futures::StreamExt,
        swarm::{dummy, NetworkBehaviour, SwarmEvent},
        Multiaddr, Swarm, SwarmBuilder,
    };
    use nomos_blend::conn_monitor::ConnectionMonitorSettings;
    use nomos_blend_message::mock::MockBlendMessage;
    use tokio::select;

    use crate::{
        behaviour::{Config, IntervalStreamProvider},
        error::Error,
        Behaviour, Event,
    };

    /// Check that a published messsage arrives in the peers successfully.
    #[tokio::test]
    async fn behaviour() {
        // Initialize two swarms that support the blend protocol.
        let addrs = build_addresses(2, 8090);
        let mut swarm1 = new_swarm(addrs[0].clone());
        let mut swarm2 = new_swarm(addrs[1].clone());
        swarm2.dial(addrs[0].clone()).unwrap();

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
                            match swarm2.behaviour_mut().publish(msg.clone()) {
                                Ok(_) => {
                                    println!("published: {:?}", msg);
                                    msg_published = true;
                                },
                                Err(e) => println!("publish error: {:?}", e),
                            }
                        }
                    }
                    // Proceed swarm1
                    event = swarm1.select_next_some() => {
                        println!("swarm1 event: {:?}", event);
                        if let SwarmEvent::Behaviour(Event::Message(received_msg)) = event {
                            assert_eq!(received_msg, msg);
                            break;
                        };
                    }
                    // Proceed swarm2
                    event = swarm2.select_next_some() => {
                        println!("swarm2 event: {:?}", event);
                    }
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
        let addrs = build_addresses(2, 9090);
        let mut swarm1 = new_swarm_without_blend(addrs[0].clone());
        let mut swarm2 = new_swarm(addrs[1].clone());
        swarm2.dial(addrs[0].clone()).unwrap();

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
        addr: Multiaddr,
    ) -> Swarm<Behaviour<MockBlendMessage, TokioIntervalStreamProvider>> {
        let mut swarm = new_swarm_with_behaviour(Behaviour::<
            MockBlendMessage,
            TokioIntervalStreamProvider,
        >::new(Config {
            duplicate_cache_lifespan: 60,
            conn_monitor_settings: Some(ConnectionMonitorSettings {
                time_window: Duration::from_secs(60),
                expected_effective_messages: U57F7::from_num(1.0),
                effective_message_tolerance: U57F7::from_num(0.1),
                expected_drop_messages: U57F7::from_num(1.0),
                drop_message_tolerance: U57F7::from_num(0.1),
            }),
        }));
        swarm.listen_on(addr).unwrap();
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
            .build()
    }

    fn build_addresses(count: usize, base_port: usize) -> Vec<Multiaddr> {
        (0..count)
            .map(|i| {
                format!("/ip4/127.0.0.1/udp/{}/quic-v1", base_port + i)
                    .parse()
                    .unwrap()
            })
            .collect()
    }

    struct TokioIntervalStreamProvider;

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
}

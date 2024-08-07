pub mod executor;
pub mod validator;

#[cfg(test)]
pub mod test {
    use crate::dispersal::executor::behaviour::DispersalExecutorBehaviour;
    use crate::dispersal::validator::behaviour::{DispersalEvent, DispersalValidatorBehaviour};
    use crate::test_utils::AllNeighbours;
    use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
    use kzgrs_backend::common::blob::DaBlob;
    use kzgrs_backend::common::Column;
    use libp2p::identity::Keypair;
    use libp2p::quic::tokio::Provider;
    use libp2p::quic::Config;
    use libp2p::swarm::SwarmEvent;
    use libp2p::{
        noise, ping, quic, tcp, yamux, Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
    };
    use log::{debug, error, trace};
    use std::pin::pin;
    use std::string::String;
    use std::time::Duration;
    use subnetworks_assignations::MembershipHandler;
    use tracing::field::debug;
    use tracing_subscriber::fmt::TestWriter;
    use tracing_subscriber::EnvFilter;

    pub fn executor_swarm(
        key: Keypair,
        membership: impl MembershipHandler<NetworkId = u32, Id = PeerId> + 'static,
    ) -> libp2p::Swarm<
        DispersalExecutorBehaviour<impl MembershipHandler<NetworkId = u32, Id = PeerId>>,
    > {
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|keypair| quic::tokio::Transport::new(quic::Config::new(keypair)))
            .unwrap()
            .with_behaviour(|_key| DispersalExecutorBehaviour::new(membership))
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
            })
            .build()
    }

    pub fn validator_swarm(
        key: Keypair,
        membership: impl MembershipHandler<NetworkId = u32, Id = PeerId> + 'static,
    ) -> libp2p::Swarm<
        DispersalValidatorBehaviour<impl MembershipHandler<NetworkId = u32, Id = PeerId>>,
    > {
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|keypair| quic::tokio::Transport::new(quic::Config::new(keypair)))
            .unwrap()
            .with_behaviour(|_key| DispersalValidatorBehaviour::new(membership))
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
            })
            .build()
    }

    #[tokio::test]
    async fn test_dispersal_single_node() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .compact()
            .with_writer(TestWriter::default())
            .try_init();
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let k2 = libp2p::identity::Keypair::generate_ed25519();
        let validator_peer = PeerId::from_public_key(&k1.public());
        let neighbours = AllNeighbours {
            neighbours: [
                PeerId::from_public_key(&k1.public()),
                PeerId::from_public_key(&k2.public()),
            ]
            .into_iter()
            .collect(),
        };
        let mut executor = executor_swarm(k1, neighbours.clone());
        let mut validator = validator_swarm(k2, neighbours);

        let msg_count = 10usize;
        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5053/quic-v1".parse().unwrap();
        let addr2 = addr.clone();
        let validator_task = async move {
            validator.listen_on(addr).unwrap();
            let mut res = vec![];
            loop {
                match validator.select_next_some().await {
                    SwarmEvent::Behaviour(DispersalEvent::IncomingMessage { message }) => {
                        res.push(message);
                    }
                    event => {
                        trace!("{event:?}");
                    }
                }
                if res.len() == msg_count {
                    break;
                }
            }
            res
        };
        let join_validator = tokio::spawn(validator_task);
        async move {
            executor.dial(addr2).unwrap();
            executor
                .behaviour_mut()
                .open_stream(validator_peer)
                .await
                .unwrap();
            for _ in 0..10 {
                executor.behaviour_mut().disperse_blob(
                    0,
                    DaBlob {
                        column: Column(vec![]),
                        column_commitment: Default::default(),
                        aggregated_column_commitment: Default::default(),
                        aggregated_column_proof: Default::default(),
                        rows_commitments: vec![],
                        rows_proofs: vec![],
                    },
                )
            }
            loop {
                tokio::select! {
                    Some(event) = executor.next() => {
                        trace!("{event:?}");
                    }
                }
            }
        }
        .await;
        assert_eq!(join_validator.await.unwrap().len(), msg_count);
    }

    #[tokio::test]
    async fn test_streams() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(TestWriter::default())
            .try_init();
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        let mut s1: Swarm<libp2p_stream::Behaviour> =
            SwarmBuilder::with_existing_identity(k1.clone())
                .with_tokio()
                .with_tcp(
                    tcp::Config::default(),
                    noise::Config::new,
                    yamux::Config::default,
                )
                .unwrap()
                .with_behaviour(|_| libp2p_stream::Behaviour::new())
                .unwrap()
                .with_swarm_config(|cfg| {
                    cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
                })
                .build();

        let mut s2: Swarm<libp2p_stream::Behaviour> =
            SwarmBuilder::with_existing_identity(k2.clone())
                .with_tokio()
                .with_tcp(
                    tcp::Config::default(),
                    noise::Config::new,
                    yamux::Config::default,
                )
                .unwrap()
                .with_behaviour(|_| libp2p_stream::Behaviour::new())
                .unwrap()
                .with_swarm_config(|cfg| {
                    cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
                })
                .build();

        let peer = PeerId::from_public_key(&k1.public());
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/3050"
            .parse::<Multiaddr>()
            .unwrap()
            .with_p2p(peer)
            .unwrap();
        let addr2 = addr.clone();
        let t1 = async move {
            s1.listen_on(addr).unwrap();
            let mut incoming_streams = s1
                .behaviour()
                .new_control()
                .accept(StreamProtocol::new("/foo"))
                .unwrap();
            // advance
            debug!("1 Advancing");
            // while let Some(event) = s1.next().await {
            //     debug!("{event:?}");
            // }
            debug!("1 waits for stream");
            let Some((p, mut s)) = incoming_streams.next().await else {
                panic!("1 No incoming stream");
            };
            debug!("1 Got a stream");
            let mut buff = b"foobar".to_vec();
            s.read_exact(&mut buff).await.unwrap();
            debug!("1 received: {}", String::from_utf8_lossy(buff.as_ref()));
            // loop {
            //     tokio::select! {
            //         _ = s.read_exact(&mut buff) => {
            //             debug!("{}", String::from_utf8_lossy(buff.as_ref()));
            //             break;
            //         }
            //         event = s1.select_next_some() => {
            //             debug!("{event:?}");
            //         }
            //     }
            // }
        };
        let j1 = tokio::spawn(t1);
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let j2 = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(2)).await;
            debug!("2 dialing");
            s2.dial(addr2.clone()).unwrap();
            let mut control = s2.behaviour().new_control();
            let mut w = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
                debug!("2 open stream");
                let mut s = control
                    .open_stream(peer, StreamProtocol::new("/foo"))
                    .await
                    .unwrap();
                debug!("2 writes");
                s.write_all(b"FooBar").await.unwrap();
                s.flush().await.unwrap();
                debug!("2 finish writing");
                tokio::time::sleep(Duration::from_secs(5)).await;
            });
            debug!("2 Advancing");
            loop {
                tokio::select! {
                    Some(event) = s2.next() => {
                        debug!("{event:?}");
                    },
                    _ = &mut receiver => {
                        break;
                    }
                    res = &mut w => {
                        debug!("Finished writting: {res:?}");
                    }
                }
            }
            debug!("2 finish");
        });
        j1.await.unwrap();
        sender.send(()).unwrap();
        j2.await.unwrap();
    }

    #[tokio::test]
    async fn test_listen() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(TestWriter::default())
            .try_init();
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        let mut s1: Swarm<_> = SwarmBuilder::with_existing_identity(k1.clone())
            .with_tokio()
            .with_other_transport(|key| quic::GenTransport::<Provider>::new(Config::new(key)))
            .unwrap()
            .with_behaviour(|_| ping::Behaviour::new(ping::Config::default()))
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
            })
            .build();

        let peer = PeerId::from_public_key(&k1.public());
        let addr: Multiaddr = "/ip4/0.0.0.0/udp/0/quic-v1".parse::<Multiaddr>().unwrap();

        s1.listen_on(addr).unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

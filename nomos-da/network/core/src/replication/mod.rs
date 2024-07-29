pub mod behaviour;
pub mod handler;

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use std::time::Duration;

    use futures::StreamExt;
    use libp2p::{Multiaddr, PeerId, Swarm, Transport, yamux};
    use libp2p::core::transport::MemoryTransport;
    use libp2p::core::upgrade::Version;
    use libp2p::identity::Keypair;
    use libp2p::swarm::SwarmEvent;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::TestWriter;

    use nomos_da_messages::common::Blob;
    use subnetworks_assignations::MembershipHandler;

    use crate::replication::behaviour::ReplicationBehaviour;
    use crate::replication::handler::DaMessage;

    #[derive(Clone)]
    struct AllNeighbours {
        neighbours: HashSet<PeerId>,
    }

    impl MembershipHandler for AllNeighbours {
        type NetworkId = u32;
        type Id = PeerId;

        fn membership(&self, self_id: &Self::Id) -> HashSet<Self::NetworkId> {
            [0].into_iter().collect()
        }

        fn members_of(&self, network_id: &Self::NetworkId) -> HashSet<Self::Id> {
            self.neighbours.clone()
        }
    }

    #[derive(libp2p::swarm::NetworkBehaviour)]
    struct TestBehaviour {
        replication_behaviour: ReplicationBehaviour<AllNeighbours>,
        // ping: PingBehaviour,
    }

    impl TestBehaviour {
        fn new(peer_id: PeerId, all_neighbours: AllNeighbours) -> Self {
            Self {
                replication_behaviour: ReplicationBehaviour::new(peer_id, all_neighbours),
                // ping: PingBehaviour::new(
                //     ping::Config::new()
                //         .with_interval(Duration::from_secs(1))
                //         .with_timeout(Duration::from_secs(10)),
                // ),
            }
        }
    }

    #[tokio::test]
    async fn test_close_connection() {
        fn get_swarm(
            key: Keypair,
            all_neighbours: AllNeighbours,
        ) -> Swarm<ReplicationBehaviour<AllNeighbours>> {
            libp2p::SwarmBuilder::with_existing_identity(key)
                .with_tokio()
                .with_other_transport(|keypair| {
                    let transport = MemoryTransport::default();
                    transport
                        .upgrade(Version::V1)
                        .authenticate(libp2p::plaintext::Config::new(keypair))
                        .multiplex(yamux::Config::default())
                        .timeout(Duration::from_secs(10))
                        .boxed()
                })
                .unwrap()
                .with_behaviour(|key| {
                    ReplicationBehaviour::new(
                        PeerId::from_public_key(&key.public()),
                        all_neighbours,
                    )
                })
                .unwrap()
                .with_swarm_config(|cfg| {
                    cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
                })
                .build()
        }
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .compact()
            .with_writer(TestWriter::default())
            .try_init();
        let k1 = libp2p::identity::Keypair::generate_ed25519();
        let k2 = libp2p::identity::Keypair::generate_ed25519();

        let neighbours = AllNeighbours {
            neighbours: [
                PeerId::from_public_key(&k1.public()),
                PeerId::from_public_key(&k2.public()),
            ]
            .into_iter()
            .collect(),
        };
        let mut swarm_1 = get_swarm(k1, neighbours.clone());
        let mut swarm_2 = get_swarm(k2, neighbours);

        let addr: Multiaddr = "/memory/6907198695372201009".parse().unwrap();
        let addr2 = addr.clone();
        let task_1 = async move {
            swarm_1.listen_on(addr).unwrap();
            loop {
                match swarm_1.select_next_some().await {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("1 - Listening on {address:?}")
                    }
                    SwarmEvent::Behaviour(event) => println!("1 - {event:?}"),
                    event => {
                        println!("1 - Swarmevent: {event:?}");
                    }
                }
            }
        };
        let join1 = tokio::spawn(task_1);

        let task_2 = async move {
            swarm_2.dial(addr2).unwrap();
            let mut i = 0usize;
            loop {
                match swarm_2.select_next_some().await {
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("2 - Listening on {address:?}")
                    }
                    SwarmEvent::Behaviour(event) => println!("2 - {event:?}"),
                    event => {
                        println!("2 - Swarmevent: {event:?}");
                    }
                }
                swarm_2.behaviour_mut().send_message(DaMessage {
                    blob: Some(Blob {
                        blob_id: vec![],
                        data: format!("Hello {i}").into_bytes(),
                    }),
                    subnetwork_id: 0,
                });
                i += 1;
            }
        };
        let join2 = tokio::spawn(task_2);
        let (r1, r2) = tokio::join!(join1, join2);
        r1.unwrap();
        r2.unwrap();
    }
}

pub mod behaviour;
pub mod handler;

#[cfg(test)]
mod test {
    use std::collections::HashSet;
    use std::time::Duration;

    use futures::StreamExt;
    use libp2p::{Multiaddr, PeerId, quic, Swarm, Transport};
    use libp2p::identity::Keypair;
    use libp2p::swarm::SwarmEvent;
    use log::{debug, info};
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt::TestWriter;

    use nomos_da_messages::common::Blob;
    use subnetworks_assignations::MembershipHandler;

    use crate::replication::behaviour::{ReplicationBehaviour, ReplicationEvent};
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

    #[tokio::test]
    async fn test_close_connection() {
        fn get_swarm(
            key: Keypair,
            all_neighbours: AllNeighbours,
        ) -> Swarm<ReplicationBehaviour<AllNeighbours>> {
            libp2p::SwarmBuilder::with_existing_identity(key)
                .with_tokio()
                .with_other_transport(|keypair| {
                    quic::tokio::Transport::new(quic::Config::new(keypair))
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

        let msg_count = 10usize;
        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5053/quic-v1".parse().unwrap();
        let addr2 = addr.clone();
        let task_1 = async move {
            swarm_1.listen_on(addr.clone()).unwrap();
            let mut res = vec![];
            loop {
                match swarm_1.select_next_some().await {
                    SwarmEvent::Behaviour(ReplicationEvent::IncomingMessage {
                        peer_id,
                        message,
                    }) => {
                        res.push(message);
                    }
                    _ => {}
                }
                if res.len() == msg_count {
                    break;
                }
            }
            // let res = swarm_1
            //     .filter_map(|event| async {
            //         if let SwarmEvent::Behaviour(ReplicationEvent::IncomingMessage {
            //             peer_id,
            //             message,
            //         }) = event
            //         {
            //             Some(message)
            //         } else {
            //             None
            //         }
            //     })
            //     .take(msg_count)
            //     .collect::<Vec<_>>()
            //     .await;
            println!("{res:?}");
            res
        };
        let join1 = tokio::spawn(task_1);
        let (mut sender, mut receiver) = tokio::sync::mpsc::channel::<()>(10);
        let (terminate_sender, mut terminate_receiver) = tokio::sync::oneshot::channel::<()>();
        let task_2 = async move {
            swarm_2.dial(addr2).unwrap();
            let mut i = 0usize;
            loop {
                tokio::select! {
                    _  = receiver.recv() => {
                        swarm_2.behaviour_mut().send_message(DaMessage {
                            blob: Some(Blob {
                                blob_id: i.to_be_bytes().to_vec(),
                                data: i.to_be_bytes().to_vec(),
                            }),
                            subnetwork_id: 0,
                        });
                        i += 1;
                    }
                    event = swarm_2.select_next_some() => {
                        match event {
                            SwarmEvent::ConnectionEstablished{ peer_id,  connection_id, .. } => {
                                info!("Connected to {peer_id} with connection_id: {connection_id}");
                            }
                            _ => {}
                        }
                    }
                    _ = &mut terminate_receiver => {
                        break;
                    }
                }
            }
        };
        let join2 = tokio::spawn(task_2);
        tokio::time::sleep(Duration::from_secs(1)).await;
        for _ in 0..10 {
            sender.send(()).await.unwrap();
        }
        loop {
            tokio::select! {
                Ok(res) = join1 => {
                    assert_eq!(res.len(), msg_count);
                    terminate_sender.send(()).unwrap();
                    break;
                }
                _ = join2 => {
                    panic!("task two never ends");
                }
            };
        }
    }
}

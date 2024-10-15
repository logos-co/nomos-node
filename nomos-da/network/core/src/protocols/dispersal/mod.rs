pub mod executor;
pub mod validator;

#[cfg(test)]
pub mod test {
    use crate::address_book::AddressBook;
    use crate::protocols::dispersal::executor::behaviour::DispersalExecutorBehaviour;
    use crate::protocols::dispersal::validator::behaviour::{
        DispersalEvent, DispersalValidatorBehaviour,
    };
    use crate::test_utils::AllNeighbours;
    use futures::StreamExt;
    use kzgrs_backend::common::blob::DaBlob;
    use kzgrs_backend::common::Column;
    use libp2p::identity::Keypair;
    use libp2p::swarm::SwarmEvent;
    use libp2p::{quic, Multiaddr, PeerId};
    use log::info;
    use std::time::Duration;
    use subnetworks_assignations::MembershipHandler;
    use tracing_subscriber::fmt::TestWriter;
    use tracing_subscriber::EnvFilter;

    pub fn executor_swarm(
        addressbook: AddressBook,
        key: Keypair,
        membership: impl MembershipHandler<NetworkId = u32, Id = PeerId> + 'static,
    ) -> libp2p::Swarm<
        DispersalExecutorBehaviour<impl MembershipHandler<NetworkId = u32, Id = PeerId>>,
    > {
        let peer_id = PeerId::from_public_key(&key.public());
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|keypair| quic::tokio::Transport::new(quic::Config::new(keypair)))
            .unwrap()
            .with_behaviour(|_key| {
                DispersalExecutorBehaviour::new(peer_id, membership, addressbook)
            })
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
        let validator_peer = PeerId::from_public_key(&k2.public());
        let neighbours = AllNeighbours {
            neighbours: [
                PeerId::from_public_key(&k1.public()),
                PeerId::from_public_key(&k2.public()),
            ]
            .into_iter()
            .collect(),
        };
        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5063/quic-v1".parse().unwrap();
        let addr2 = addr.clone().with_p2p(validator_peer).unwrap();
        let addressbook =
            AddressBook::from_iter([(PeerId::from_public_key(&k2.public()), addr2.clone())]);
        let mut executor = executor_swarm(addressbook, k1, neighbours.clone());
        let mut validator = validator_swarm(k2, neighbours);

        let msg_count = 10usize;

        let validator_task = async move {
            validator.listen_on(addr).unwrap();
            let mut res = vec![];
            loop {
                match validator.select_next_some().await {
                    SwarmEvent::Behaviour(DispersalEvent::IncomingMessage { message }) => {
                        res.push(message);
                    }
                    event => {
                        info!("Validator event: {event:?}");
                    }
                }
                if res.len() == msg_count {
                    break;
                }
            }
            res
        };
        let join_validator = tokio::spawn(validator_task);
        tokio::time::sleep(Duration::from_secs(1)).await;
        executor.dial(addr2).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let executor_open_stream_sender = executor.behaviour().open_stream_sender();
        let executor_disperse_blob_sender = executor.behaviour().blobs_sender();
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let executor_poll = async move {
            loop {
                tokio::select! {
                    Some(event) = executor.next() => {
                        info!("Executor event: {event:?}");
                    }
                    _ = &mut receiver => {
                        break;
                    }
                }
            }
        };
        let executor_task = tokio::spawn(executor_poll);
        executor_open_stream_sender.send(validator_peer).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        for i in 0..10 {
            info!("Sending blob: {i}");
            executor_disperse_blob_sender
                .send((
                    0,
                    DaBlob {
                        column_idx: 0,
                        column: Column(vec![]),
                        column_commitment: Default::default(),
                        aggregated_column_commitment: Default::default(),
                        aggregated_column_proof: Default::default(),
                        rows_commitments: vec![],
                        rows_proofs: vec![],
                    },
                ))
                .unwrap()
        }

        assert_eq!(join_validator.await.unwrap().len(), msg_count);
        sender.send(()).unwrap();
        executor_task.await.unwrap();
    }
}

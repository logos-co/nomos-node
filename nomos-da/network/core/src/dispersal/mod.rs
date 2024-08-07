pub mod executor;
pub mod validator;

#[cfg(test)]
pub mod test {
    use crate::dispersal::executor::behaviour::{
        DispersalExecutorBehaviour, DispersalExecutorEvent,
    };
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
    use log::{debug, error, info, trace};
    use std::pin::pin;
    use std::string::String;
    use std::time::Duration;
    use subnetworks_assignations::MembershipHandler;
    use tracing::field::debug;
    use tracing::instrument::WithSubscriber;
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
        let mut executor_open_stream_sender = executor.behaviour().open_stream_sender();
        let mut executor_disperse_blob_sender = executor.behaviour().blobs_sender();
        let (sender, mut receiver) = tokio::sync::oneshot::channel();
        let executor_poll = async move {
            executor.dial(addr2).unwrap();
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
        for _ in 0..10 {
            executor_disperse_blob_sender
                .send((
                    0,
                    DaBlob {
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

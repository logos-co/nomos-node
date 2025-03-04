pub mod executor;
pub mod validator;

#[cfg(test)]
pub mod test {
    use futures::StreamExt;
    use kzgrs_backend::common::{blob::DaBlob, Column};
    use libp2p::Transport;
    use libp2p::{identity::Keypair, swarm::SwarmEvent, Multiaddr, PeerId, swarm::NetworkBehaviour};
    use libp2p::core::{transport::MemoryTransport, upgrade::Version};
    use std::time::Duration;
    use log::info;
    use tracing_subscriber::{fmt::TestWriter, EnvFilter};

    use crate::{
        address_book::AddressBook,
        protocols::dispersal::{
            executor::behaviour::DispersalExecutorBehaviour,
            validator::behaviour::{DispersalEvent, DispersalValidatorBehaviour},
        },
        test_utils::AllNeighbours,
    };

    pub fn new_in_memory<TBehavior>(
        key: Keypair,
        behavior: TBehavior,
    ) -> libp2p::Swarm<TBehavior>
    where
        TBehavior: NetworkBehaviour + Send,
    {        
        libp2p::SwarmBuilder::with_existing_identity(key.clone())
            .with_tokio()
            .with_other_transport(|_| {
                let transport = MemoryTransport::default()
                    .upgrade(Version::V1)
                    .authenticate(libp2p::plaintext::Config::new(&key))
                    .multiplex(libp2p::yamux::Config::default())
                    .timeout(Duration::from_secs(20));
                   
                Ok(transport)
            })
            .unwrap()
            .with_behaviour(|_| behavior)  
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
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
        let addr: Multiaddr = "/memory/1".parse().unwrap();
        let addr2 = addr.clone().with_p2p(validator_peer).unwrap();
        let addressbook =
            AddressBook::from_iter([(PeerId::from_public_key(&k2.public()), addr2.clone())]);

        let executor_behavior = DispersalExecutorBehaviour::new(
            PeerId::from_public_key(&k1.public()),
            neighbours.clone(),
            addressbook
        );

        let mut executor = new_in_memory(k1, executor_behavior);

        let validator_behavior  = DispersalValidatorBehaviour::new(
            neighbours.clone(),
        );

        let mut validator = new_in_memory(k2, validator_behavior);

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

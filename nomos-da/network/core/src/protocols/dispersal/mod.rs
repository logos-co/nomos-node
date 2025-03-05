pub mod executor;
pub mod validator;

#[cfg(test)]
pub mod test {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
    };

    use futures::StreamExt;
    use kzgrs_backend::common::{blob::DaBlob, Column};
    use libp2p::{
        swarm::{dial_opts::DialOpts, SwarmEvent},
        PeerId, Swarm,
    };
    use libp2p_swarm_test::SwarmExt;
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

    #[tokio::test]
    async fn test_dispersal_single_node() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .compact()
            .with_writer(TestWriter::default())
            .try_init();

        let neighbours = AllNeighbours {
            neighbours: Arc::new(Mutex::new(HashSet::new())),
        };

        let mut executor = Swarm::new_ephemeral_tokio(|k1| {
            let p1 = PeerId::from_public_key(&k1.public());
            neighbours.neighbours.lock().unwrap().insert(p1);
            DispersalExecutorBehaviour::new(neighbours.clone(), AddressBook::empty())
        });
        let mut validator = Swarm::new_ephemeral_tokio(|k2| {
            let p2 = PeerId::from_public_key(&k2.public());
            neighbours.neighbours.lock().unwrap().insert(p2);
            DispersalValidatorBehaviour::new(neighbours.clone())
        });

        validator.listen().with_memory_addr_external().await;
        executor
            .dial(
                DialOpts::peer_id(*validator.local_peer_id())
                    .addresses(validator.external_addresses().cloned().collect())
                    .build(),
            )
            .unwrap();

        let msg_count = 10usize;

        let validator_task = async move {
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

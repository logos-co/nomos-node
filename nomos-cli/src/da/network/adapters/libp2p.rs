// std
use std::collections::HashSet;
// crates
use futures::{future::join_all, StreamExt};
use kzgrs_backend::{common::blob::DaBlob, encoder::EncodedData as KzgEncodedData};
use nomos_core::da::DaDispersal;
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::{DaNetworkMsg, NetworkService};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use thiserror::Error;
use tokio::sync::oneshot;
// internal
use crate::da::{
    network::{backend::Command, swarm::DispersalEvent},
    NetworkBackend,
};

type Relay<T> = OutboundRelay<<NetworkService<T> as ServiceData>::Message>;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct DispersalError(String);

impl From<String> for DispersalError {
    fn from(s: String) -> Self {
        DispersalError(s)
    }
}

pub struct Libp2pExecutorDispersalAdapter {
    network_relay: Relay<NetworkBackend>,
}

impl Libp2pExecutorDispersalAdapter {
    pub fn new(network_relay: Relay<NetworkBackend>) -> Self {
        Self { network_relay }
    }
}

#[async_trait::async_trait]
impl DaDispersal for Libp2pExecutorDispersalAdapter {
    type EncodedData = KzgEncodedData;
    type Error = DispersalError;

    async fn disperse(&self, encoded_data: Self::EncodedData) -> Result<(), Self::Error> {
        let mut tasks = Vec::new();

        let (sender, receiver) = oneshot::channel();
        self.network_relay
            .send(DaNetworkMsg::Subscribe { kind: (), sender })
            .await
            .map_err(|(e, _)| e.to_string())?;
        let mut event_stream = receiver.await.map_err(|e| e.to_string())?;
        let mut expected_acknowledgments = HashSet::new();

        for (i, column) in encoded_data.extended_data.columns().enumerate() {
            let blob = DaBlob {
                column: column.clone(),
                column_idx: i
                    .try_into()
                    .expect("Column index shouldn't overflow the target type"),
                column_commitment: encoded_data.column_commitments[i],
                aggregated_column_commitment: encoded_data.aggregated_column_commitment,
                aggregated_column_proof: encoded_data.aggregated_column_proofs[i],
                rows_commitments: encoded_data.row_commitments.clone(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|proofs| proofs.get(i).cloned().unwrap())
                    .collect(),
            };

            expected_acknowledgments.insert((blob.id().clone(), i as SubnetworkId));

            let relay = self.network_relay.clone();
            let command = DaNetworkMsg::Process(Command::Disperse {
                blob,
                subnetwork_id: i as u32,
            });

            let task = async move { relay.send(command).await.map_err(|(e, _)| e.to_string()) };

            tasks.push(task);
        }

        let results: Vec<Result<(), String>> = join_all(tasks).await;

        for result in results {
            result?;
        }

        while !expected_acknowledgments.is_empty() {
            let event = event_stream.next().await;
            match event {
                Some(event) => match event {
                    DispersalEvent::DispersalSuccess {
                        blob_id,
                        subnetwork_id,
                    } => {
                        expected_acknowledgments.remove(&(blob_id.to_vec(), subnetwork_id));
                    }
                    DispersalEvent::DispersalError { error } => {
                        return Err(DispersalError(format!("Received dispersal error: {error}")));
                    }
                },
                None => {
                    return Err(DispersalError(
                        "Event stream ended before receiving all acknowledgments".into(),
                    ));
                }
            }
        }

        Ok(())
    }
}

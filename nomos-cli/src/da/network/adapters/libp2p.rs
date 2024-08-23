// std
// crates
use kzgrs_backend::{common::blob::DaBlob, encoder::EncodedData as KzgEncodedData};
use nomos_core::da::DaDispersal;
use nomos_da_network_service::{DaNetworkMsg, NetworkService};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use thiserror::Error;
// internal
use crate::da::{network::backend::Command, NetworkBackend};

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

            self.network_relay
                .send(DaNetworkMsg::Process(Command::Disperse {
                    blob,
                    subnetwork_id: i as u32,
                }))
                .await
                .map_err(|(e, _)| e.to_string())?
        }
        Ok(())
    }
}

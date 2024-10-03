use crate::adapters::network::DispersalNetworkAdapter;
use crate::backend::DispersalBackend;
use futures::StreamExt;
use itertools::izip;
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::common::{build_blob_id, Column, ColumnIndex};
use kzgrs_backend::encoder;
use kzgrs_backend::encoder::EncodedData;
use nomos_core::da::{BlobId, DaDispersal, DaEncoder};
use overwatch_rs::DynError;
use std::sync::Arc;
use std::time::Duration;

pub struct DispersalKZGRSBackendSettings {
    encoder_settings: encoder::DaEncoderParams,
    dispersal_timeout: Duration,
}
pub struct DispersalKZGRSBackend<NetworkAdapter> {
    settings: DispersalKZGRSBackendSettings,
    adapter: Arc<NetworkAdapter>,
    encoder: Arc<encoder::DaEncoder>,
}

pub struct DispersalFromAdapter<Adapter> {
    adapter: Arc<Adapter>,
    timeout: Duration,
}

#[async_trait::async_trait]
impl<Adapter> DaDispersal for DispersalFromAdapter<Adapter>
where
    Adapter: DispersalNetworkAdapter + Send + Sync,
    Adapter::SubnetworkId: From<usize> + Send + Sync,
{
    type EncodedData = EncodedData;
    type Error = DynError;

    async fn disperse(&self, encoded_data: Self::EncodedData) -> Result<(), Self::Error> {
        let adapter = self.adapter.as_ref();
        let encoded_size = encoded_data.extended_data.len();
        let blob_id = build_blob_id(
            &encoded_data.aggregated_column_commitment,
            &encoded_data.row_commitments,
        );

        let reponses_stream = adapter.dispersal_events_stream().await?;
        for (subnetwork_id, blob) in encoded_data_to_da_blobs(encoded_data).enumerate() {
            adapter.disperse(subnetwork_id.into(), blob).await?;
        }

        let valid_responses = reponses_stream
            .filter_map(|event| async move {
                match event {
                    Ok((_blob_id, _)) if _blob_id == blob_id => Some(()),
                    _ => None,
                }
            })
            .take(encoded_size)
            .collect();
        // timeout when collecting positive responses
        tokio::time::timeout(self.timeout, valid_responses)
            .await
            .map_err(|e| Box::new(e) as DynError)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl<NetworkAdapter> DispersalBackend for DispersalKZGRSBackend<NetworkAdapter>
where
    NetworkAdapter: DispersalNetworkAdapter + Send + Sync,
    NetworkAdapter::SubnetworkId: From<usize> + Send + Sync,
{
    type Settings = DispersalKZGRSBackendSettings;
    type Encoder = encoder::DaEncoder;
    type Dispersal = DispersalFromAdapter<NetworkAdapter>;
    type Adapter = NetworkAdapter;
    type BlobId = BlobId;

    fn init(settings: Self::Settings, adapter: Self::Adapter) -> Self {
        let encoder = Self::Encoder::new(settings.encoder_settings.clone());
        Self {
            settings,
            adapter: Arc::new(adapter),
            encoder: Arc::new(encoder),
        }
    }

    async fn encode(
        &self,
        data: Vec<u8>,
    ) -> Result<(Self::BlobId, <Self::Encoder as DaEncoder>::EncodedData), DynError> {
        let encoder = Arc::clone(&self.encoder);
        let heavy_task = tokio::task::spawn_blocking(move || encoder.encode(&data));
        let encoded_data = heavy_task.await??;
        let blob_id = build_blob_id(
            &encoded_data.aggregated_column_commitment,
            &encoded_data.row_commitments,
        );
        Ok((blob_id, encoded_data))
    }

    async fn disperse(
        &self,
        encoded_data: <Self::Encoder as DaEncoder>::EncodedData,
    ) -> Result<(), DynError> {
        DispersalFromAdapter {
            adapter: Arc::clone(&self.adapter),
            timeout: self.settings.dispersal_timeout,
        }
        .disperse(encoded_data)
        .await
    }

    async fn publish_to_mempool(&self, blob_id: Self::BlobId) -> Result<(), DynError> {
        todo!()
    }

    async fn process_dispersal(&self, data: Vec<u8>) -> Result<(), DynError> {
        todo!()
    }
}

fn encoded_data_to_da_blobs(encoded_data: EncodedData) -> impl Iterator<Item = DaBlob> {
    let EncodedData {
        extended_data,
        row_commitments,
        rows_proofs,
        column_commitments,
        aggregated_column_commitment,
        aggregated_column_proofs,
        ..
    } = encoded_data;
    let iter = izip!(
        // transpose and unwrap the types as we need to have ownership of it
        extended_data.transposed().0.into_iter().map(|r| r.0),
        column_commitments.into_iter(),
        aggregated_column_proofs.into_iter(),
    );
    iter.enumerate().map(
        move |(column_idx, (column, column_commitment, aggregated_column_proof))| DaBlob {
            column: Column(column),
            column_idx: column_idx as ColumnIndex,
            column_commitment,
            aggregated_column_commitment,
            aggregated_column_proof,
            rows_commitments: row_commitments.clone(),
            rows_proofs: rows_proofs
                .iter()
                .map(|proofs| proofs[column_idx].clone())
                .collect(),
        },
    )
}

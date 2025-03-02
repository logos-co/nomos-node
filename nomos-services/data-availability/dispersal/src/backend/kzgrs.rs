use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use kzgrs_backend::{
    common::build_blob_id,
    dispersal, encoder,
    encoder::{DaEncoderParams, EncodedData},
};
use nomos_core::da::{BlobId, DaDispersal, DaEncoder};
use overwatch_rs::DynError;
use serde::{Deserialize, Serialize};

use crate::{
    adapters::{mempool::DaMempoolAdapter, network::DispersalNetworkAdapter},
    backend::DispersalBackend,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncoderSettings {
    pub num_columns: usize,
    pub with_cache: bool,
    pub global_params_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispersalKZGRSBackendSettings {
    pub encoder_settings: EncoderSettings,
    pub dispersal_timeout: Duration,
}
pub struct DispersalKZGRSBackend<NetworkAdapter, MempoolAdapter> {
    settings: DispersalKZGRSBackendSettings,
    network_adapter: Arc<NetworkAdapter>,
    mempool_adapter: MempoolAdapter,
    encoder: Arc<encoder::DaEncoder>,
}

pub struct DispersalFromAdapter<Adapter> {
    adapter: Arc<Adapter>,
    timeout: Duration,
}

// remove if solved, this occurs in the timeout method below (out of our
// handling)
#[expect(dependency_on_unit_never_type_fallback)]
#[async_trait::async_trait]
impl<Adapter> DaDispersal for DispersalFromAdapter<Adapter>
where
    Adapter: DispersalNetworkAdapter + Send + Sync,
    Adapter::SubnetworkId: From<u16> + Send + Sync,
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

        let responses_stream = adapter.dispersal_events_stream().await?;
        for (subnetwork_id, blob) in encoded_data.into_iter().enumerate() {
            adapter
                .disperse((subnetwork_id as u16).into(), blob)
                .await?;
        }

        let valid_responses = responses_stream
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
impl<NetworkAdapter, MempoolAdapter> DispersalBackend
    for DispersalKZGRSBackend<NetworkAdapter, MempoolAdapter>
where
    NetworkAdapter: DispersalNetworkAdapter + Send + Sync,
    NetworkAdapter::SubnetworkId: From<u16> + Send + Sync,
    MempoolAdapter: DaMempoolAdapter<BlobId = BlobId, Metadata = dispersal::Metadata> + Send + Sync,
{
    type Settings = DispersalKZGRSBackendSettings;
    type Encoder = encoder::DaEncoder;
    type Dispersal = DispersalFromAdapter<NetworkAdapter>;
    type NetworkAdapter = NetworkAdapter;
    type MempoolAdapter = MempoolAdapter;
    type Metadata = dispersal::Metadata;
    type BlobId = BlobId;

    fn init(
        settings: Self::Settings,
        network_adapter: Self::NetworkAdapter,
        mempool_adapter: Self::MempoolAdapter,
    ) -> Self {
        let encoder_settings = &settings.encoder_settings;
        let global_params = kzgrs_backend::global::global_parameters_from_file(
            &encoder_settings.global_params_path,
        )
        .expect("Global encoder params should be available");
        let encoder = Self::Encoder::new(DaEncoderParams::new(
            encoder_settings.num_columns,
            encoder_settings.with_cache,
            global_params,
        ));
        Self {
            settings,
            network_adapter: Arc::new(network_adapter),
            mempool_adapter,
            encoder: Arc::new(encoder),
        }
    }

    async fn encode(
        &self,
        data: Vec<u8>,
    ) -> Result<(Self::BlobId, <Self::Encoder as DaEncoder>::EncodedData), DynError> {
        let encoder = Arc::clone(&self.encoder);
        // this is a REALLY heavy task, so we should try not to block the thread here
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
            adapter: Arc::clone(&self.network_adapter),
            timeout: self.settings.dispersal_timeout,
        }
        .disperse(encoded_data)
        .await
    }

    async fn publish_to_mempool(
        &self,
        blob_id: Self::BlobId,
        metadata: Self::Metadata,
    ) -> Result<(), DynError> {
        self.mempool_adapter.post_blob_id(blob_id, metadata).await
    }
}

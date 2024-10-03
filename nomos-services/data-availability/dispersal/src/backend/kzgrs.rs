use crate::adapters::network::DispersalNetworkAdapter;
use crate::backend::DispersalBackend;
use kzgrs_backend::encoder;
use kzgrs_backend::encoder::EncodedData;
use nomos_core::da::{BlobId, DaDispersal, DaEncoder};
use overwatch_rs::DynError;
use std::sync::Arc;

pub struct DispersalKZGRSBackendSettings {
    encoder_settings: encoder::DaEncoderParams,
}
pub struct DispersalKZGRSBackend<NetworkAdapter> {
    settings: DispersalKZGRSBackendSettings,
    adapter: Arc<NetworkAdapter>,
}

pub struct DispersalFromAdapter<Adapter>(Arc<Adapter>);

#[async_trait::async_trait]
impl<Adapter> DaDispersal for DispersalFromAdapter<Adapter>
where
    Adapter: Send + Sync,
{
    type EncodedData = EncodedData;
    type Error = DynError;

    async fn disperse(&self, encoded_data: Self::EncodedData) -> Result<(), Self::Error> {
        let adapter = self.0.as_ref();

        Ok(())
    }
}

#[async_trait::async_trait]
impl<NetworkAdapter> DispersalBackend for DispersalKZGRSBackend<NetworkAdapter>
where
    NetworkAdapter: DispersalNetworkAdapter + Send + Sync,
{
    type Settings = DispersalKZGRSBackendSettings;
    type Encoder = encoder::DaEncoder;
    type Dispersal = DispersalFromAdapter<NetworkAdapter>;
    type Adapter = NetworkAdapter;
    type BlobId = BlobId;

    fn init(config: Self::Settings, adapter: Self::Adapter) -> Self {
        todo!()
    }

    async fn encode(
        &self,
        data: Vec<u8>,
    ) -> Result<(Self::BlobId, <Self::Encoder as DaEncoder>::EncodedData), DynError> {
        todo!()
    }

    async fn disperse(
        &self,
        encoded_data: <Self::Encoder as DaEncoder>::EncodedData,
    ) -> Result<(), DynError> {
        todo!()
    }

    async fn publish_to_mempool(&self, blob_id: Self::BlobId) -> Result<(), DynError> {
        todo!()
    }

    async fn process_dispersal(&self, data: Vec<u8>) -> Result<(), DynError> {
        todo!()
    }
}

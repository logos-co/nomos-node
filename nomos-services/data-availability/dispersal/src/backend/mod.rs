use crate::adapters::{mempool::DaMempoolAdapter, network::DispersalNetworkAdapter};

use nomos_core::da::{DaDispersal, DaEncoder};
use overwatch_rs::DynError;

pub mod kzgrs;

#[async_trait::async_trait]
pub trait DispersalBackend {
    type Settings;
    type Encoder: DaEncoder;
    type Dispersal: DaDispersal<EncodedData = <Self::Encoder as DaEncoder>::EncodedData>;
    type NetworkAdapter: DispersalNetworkAdapter;
    type MempoolAdapter: DaMempoolAdapter;
    type BlobId: Send;

    fn init(
        config: Self::Settings,
        network_adapter: Self::NetworkAdapter,
        mempool_adapter: Self::MempoolAdapter,
    ) -> Self;
    async fn encode(
        &self,
        data: Vec<u8>,
    ) -> Result<(Self::BlobId, <Self::Encoder as DaEncoder>::EncodedData), DynError>;
    async fn disperse(
        &self,
        encoded_data: <Self::Encoder as DaEncoder>::EncodedData,
    ) -> Result<(), DynError>;

    async fn publish_to_mempool(&self, blob_id: Self::BlobId) -> Result<(), DynError>;

    async fn process_dispersal(&self, data: Vec<u8>) -> Result<(), DynError> {
        let (blob_id, encoded_data) = self.encode(data).await?;
        self.disperse(encoded_data).await?;
        self.publish_to_mempool(blob_id).await?;
        Ok(())
    }
}
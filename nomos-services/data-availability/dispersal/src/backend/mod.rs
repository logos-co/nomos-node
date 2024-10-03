use crate::adapters::network::DispersalNetworkAdapter;
use nomos_core::da::{BlobId, DaDispersal, DaEncoder};
use overwatch_rs::overwatch::handle::OverwatchHandle;

pub mod kzgrs;

#[async_trait::async_trait]
pub trait DispersalBackend {
    type Settings;
    type Encoder: DaEncoder;
    type Dispersal: DaDispersal<EncodedData = <Self::Encoder as DaEncoder>::EncodedData>;
    type Adapter: DispersalNetworkAdapter;
    type BlobId;

    type Error;

    fn init(config: Self::Settings, adapter: Self::Adapter) -> Self;
    async fn encode(
        &self,
        data: Vec<u8>,
    ) -> Result<<Self::Encoder as DaEncoder>::EncodedData, Self::Error>;
    async fn disperse(
        &self,
        network_adapter: &Self::Adapter,
    ) -> Result<<Self::Dispersal as DaDispersal>::EncodedData, Self::Error>;

    async fn publish_to_mempool(&self, blob_id: BlobId) -> Result<(), Self::Error>;
}

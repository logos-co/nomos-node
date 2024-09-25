use nomos_core::da::{BlobId, DaDispersal, DaEncoder};
use overwatch_rs::overwatch::handle::OverwatchHandle;

mod kzgrs;

#[async_trait::async_trait]
trait DispersalBackend {
    type Encoder: DaEncoder;
    type Dispersal: DaDispersal<EncodedData = <Self::Encoder as DaEncoder>::EncodedData>;
    type BlobId;

    type Error;

    fn init(overwatch_handle: OverwatchHandle) -> Self;
    async fn encode(
        data: Vec<u8>,
    ) -> Result<<Self::Encoder as DaEncoder>::EncodedData, Self::Error>;
    async fn disperse() -> Result<<Self::Dispersal as DaDispersal>::EncodedData, Self::Error>;

    async fn publish_to_mempool(blob_id: BlobId) -> Result<(), Self::Error>;
}

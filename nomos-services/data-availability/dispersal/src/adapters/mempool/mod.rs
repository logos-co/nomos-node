pub mod kzgrs;

use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;

#[async_trait::async_trait]
pub trait DaMempoolAdapter {
    type MempoolService: ServiceData;
    type BlobId;
    type Metadata;

    fn new(outbound_relay: OutboundRelay<<Self::MempoolService as ServiceData>::Message>) -> Self;

    async fn post_blob_id(
        &self,
        blob_id: Self::BlobId,
        metadata: Self::Metadata,
    ) -> Result<(), DynError>;
}

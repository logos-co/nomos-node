use nomos_core::da::BlobId;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;

pub trait MempoolAdapter {
    type MempoolService: ServiceData;
    type BlobId;

    fn new(outbound_relay: OutboundRelay<<Self::MempoolService as ServiceData>::Message>) -> Self;

    async fn post_blob_id(&self, blob_id: Self::BlobId) -> Result<(), DynError>;
}

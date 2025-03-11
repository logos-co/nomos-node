use async_trait::async_trait;
use overwatch_rs::DynError;
use tokio::sync::oneshot;

pub mod http;

/// Trait to support `Nomos` API requests
#[async_trait]
pub trait ApiAdapter {
    type Settings;
    type Blob;
    type BlobId;
    type Commitments;
    fn new(settings: Self::Settings) -> Self;
    async fn request_commitments(
        &self,
        request: Self::BlobId,
        reply_channel: oneshot::Sender<Option<Self::Commitments>>,
    ) -> Result<(), DynError>;
}

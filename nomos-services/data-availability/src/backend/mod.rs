mod memory_cache;

use nomos_core::blob::Blob;
use overwatch_rs::DynError;

#[derive(Debug)]
pub enum DaError {
    Dyn(DynError),
}

#[async_trait::async_trait]
pub trait DaBackend {
    type Settings: Clone;

    type Blob: Blob;

    fn new(settings: Self::Settings) -> Self;

    async fn add_blob(&mut self, blob: Self::Blob) -> Result<(), DaError>;

    async fn pending_blobs(&self) -> Box<dyn Iterator<Item = Self::Blob> + Send>;
}

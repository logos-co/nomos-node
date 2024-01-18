pub mod memory_cache;

use futures::Future;
use nomos_core::da::blob::Blob;
use overwatch_rs::DynError;

#[derive(Debug)]
pub enum DaError {
    Dyn(DynError),
}

pub trait DaBackend: Send {
    type Settings: Send + Clone;

    type Blob: Blob;

    fn new(settings: Self::Settings) -> Self;

    fn add_blob(&self, blob: Self::Blob) -> impl Future<Output = Result<(), DaError>> + Send;

    fn remove_blob(
        &self,
        blob: &<Self::Blob as Blob>::Hash,
    ) -> impl Future<Output = Result<(), DaError>> + Send;

    fn get_blob(&self, id: &<Self::Blob as Blob>::Hash) -> Option<Self::Blob>;
}

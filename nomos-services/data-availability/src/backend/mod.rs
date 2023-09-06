use overwatch_rs::DynError;

#[derive(Debug)]
pub enum DaError {
    Dyn(DynError),
}

pub trait DaBackend {
    type Settings: Clone;

    type Blob;

    fn new(settings: Self::Settings) -> Self;

    fn add_blob(&mut self, blob: Self::Blob) -> Result<(), DaError>;

    fn pending_blobs(&self) -> Box<dyn Iterator<Item = Self::Blob> + Send>;
}

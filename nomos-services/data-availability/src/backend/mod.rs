pub enum DaError {}
pub trait DaBackend {
    type Settings: Clone;

    type Blob;

    fn new(settings: Self::Settings) -> Self;

    fn add_blob(blob: Self::Blob) -> Result<(), DaError>;

    fn pending_blobs() -> Box<dyn Iterator<Item = Self::Blob> + Send>;
}

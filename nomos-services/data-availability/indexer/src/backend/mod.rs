pub trait DaStorageBackend {
    type Settings: Clone;

    type Blob;
    type VID;

    fn new(settings: Self::Settings) -> Self;
}

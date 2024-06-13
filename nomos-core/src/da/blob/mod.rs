pub trait Blob {
    type BlobId;

    fn id(&self) -> Self::BlobId;
}

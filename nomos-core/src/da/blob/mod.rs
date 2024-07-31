pub mod metadata;
pub mod select;
pub mod vid;

pub trait Blob {
    type BlobId;

    fn id(&self) -> Self::BlobId;
}

pub trait BlobSelect {
    type BlobId: vid::DispersedBlobData;
    type Settings: Clone;

    fn new(settings: Self::Settings) -> Self;
    fn select_blob_from<'i, I: Iterator<Item = Self::BlobId> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::BlobId> + 'i;
}

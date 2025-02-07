pub mod info;
pub mod metadata;
pub mod select;

pub trait Blob {
    type BlobId;
    type ColumnIndex;

    fn id(&self) -> Self::BlobId;
    fn column_idx(&self) -> Self::ColumnIndex;
}

pub trait BlobSelect {
    type BlobId: info::DispersedBlobInfo;
    type Settings: Clone;

    fn new(settings: Self::Settings) -> Self;
    fn select_blob_from<'i, I: Iterator<Item = Self::BlobId> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::BlobId> + 'i;
}

pub mod info;
pub mod metadata;
pub mod select;

pub trait Blob {
    type BlobId;
    type ColumnIndex;
    type LightBlob;
    type SharedCommitments;

    fn id(&self) -> Self::BlobId;
    fn column_idx(&self) -> Self::ColumnIndex;
    fn into_blob_and_shared_commitments(self) -> (Self::LightBlob, Self::SharedCommitments);

    fn from_blob_and_shared_commitments(
        light_blob: Self::LightBlob,
        shared_commitments: Self::SharedCommitments,
    ) -> Self;
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

use super::metadata::Metadata;

pub trait DispersedBlobData: Metadata {
    type BlobId;

    fn blob_id(&self) -> Self::BlobId;
    fn size(&self) -> usize;
}

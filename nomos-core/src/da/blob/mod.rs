pub trait Blob {
    type BlobId;
    type ColumnId;

    fn id(&self) -> Self::BlobId;
    fn column_id(&self) -> Self::ColumnId;
}

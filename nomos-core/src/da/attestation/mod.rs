use crate::da::blob::Blob;
use bytes::Bytes;

pub trait Attestation {
    type Blob: Blob;
    fn blob(&self) -> <Self::Blob as Blob>::Hash;
    fn as_bytes(&self) -> Bytes;
}

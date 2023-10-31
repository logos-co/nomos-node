use crate::da::blob::Blob;
use bytes::Bytes;
use std::hash::Hash;

pub trait Attestation {
    type Blob: Blob;
    type Hash: Hash + Eq + Clone;
    fn blob(&self) -> <Self::Blob as Blob>::Hash;
    fn hash(&self) -> Self::Hash;
    fn as_bytes(&self) -> Bytes;
}

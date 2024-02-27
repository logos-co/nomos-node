use crate::da::blob::Blob;
use bytes::Bytes;
use std::hash::Hash;

pub trait Attestation {
    type Blob: Blob;
    type Hash: Hash + Eq + Clone;
    type Voter;
    type Signature;
    fn blob(&self) -> <Self::Blob as Blob>::Hash;
    fn hash(&self) -> Self::Hash;
    fn voter(&self) -> Self::Voter;
    fn as_bytes(&self) -> Bytes;
    fn signature(&self) -> Option<Self::Signature>;
}

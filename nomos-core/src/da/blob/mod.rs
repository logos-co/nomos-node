use bytes::Bytes;
use std::hash::Hash;

pub type BlobHasher<T> = fn(&T) -> <T as Blob>::Hash;

pub trait Blob {
    const HASHER: BlobHasher<Self>;
    type Hash: Hash + Eq + Clone;
    /// Identifier of the blob producer.
    type Sender: Eq + Clone;

    fn hash(&self) -> Self::Hash {
        Self::HASHER(self)
    }
    fn as_bytes(&self) -> Bytes;
    fn sender(&self) -> Self::Sender;
    fn verify(&self) -> bool;
}

pub mod select;

use bytes::Bytes;
use std::hash::Hash;

pub type BlobHasher<T> = fn(&T) -> <T as Blob>::Hash;

pub trait Blob {
    const HASHER: BlobHasher<Self>;
    type Hash: Hash + Eq + Clone;
    fn hash(&self) -> Self::Hash {
        Self::HASHER(self)
    }
    fn as_bytes(&self) -> Bytes;
}

pub trait BlobSelect {
    type Blob: Blob;
    fn select_blob_from<'i, I: Iterator<Item = Self::Blob> + 'i>(
        &self,
        blobs: I,
    ) -> Box<dyn Iterator<Item = Self::Blob> + 'i>;
}

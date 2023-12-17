use bytes::Bytes;
use std::hash::Hash;

pub type BlobHasher<T> = fn(&T) -> <T as Blob>::Hash;

pub trait BlobAuth {
    type PublicKey;
    type Signature;

    fn public_key(&self) -> Self::PublicKey;
    fn sign<H>(&self, hash: H) -> Self::Signature;
}

pub trait Blob {
    const HASHER: BlobHasher<Self>;
    type Hash: Hash + Eq + Clone;
    type Sender;
    type Signature;

    fn hash(&self) -> Self::Hash {
        Self::HASHER(self)
    }
    fn as_bytes(&self) -> Bytes;
    fn sender(&self) -> Self::Sender;
    fn signature(&self) -> Self::Signature;
    fn verify(&self) -> bool;
}

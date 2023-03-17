use std::hash::Hash;
// std
// crates
use bytes::Bytes;
// internal

pub mod carnot;
#[cfg(feature = "mock")]
pub mod mock;

pub type TransactionHasher<T> = fn(&T) -> <T as Transaction>::Hash;

pub trait Transaction {
    const HASHER: TransactionHasher<Self>;
    type Hash: Hash + Eq + Clone;
    fn hash(&self) -> Self::Hash {
        Self::HASHER(self)
    }
    fn as_bytes(&self) -> Bytes;
}

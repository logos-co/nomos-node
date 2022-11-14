pub mod sled;

// std
use std::error::Error;
// crates
use bytes::Bytes;
// internal

pub trait StorageBackend {
    type Settings: Clone + Send + Sync + 'static;
    type Error: Error;
    type Transaction: Send + Sync;
    fn new(config: Self::Settings) -> Self;
    fn store<K: AsRef<[u8]>, T: Into<Bytes>>(&self, key: K, value: T) -> Result<(), Self::Error>;
    fn load<K: AsRef<[u8]>, T: From<Bytes> + Sized>(&self, key: &K) -> Result<T, Self::Error>;
    fn remove<K: AsRef<[u8]>, T: From<Bytes>>(&self, key: &K) -> Result<Option<T>, Self::Error>;
    fn execute(&self, transaction: Self::Transaction) -> Result<(), Self::Error>;
}

pub mod sled;

// std
use async_trait::async_trait;
use std::error::Error;
// crates
use bytes::Bytes;
// internal

#[async_trait]
pub trait StorageBackend {
    type Settings: Clone + Send + Sync + 'static;
    type Error: Error + 'static;
    type Transaction: Send + Sync;
    fn new(config: Self::Settings) -> Self;
    async fn store<K: AsRef<[u8]>, T: Into<Bytes>>(
        &self,
        key: K,
        value: T,
    ) -> Result<(), Self::Error>;
    async fn load<K: AsRef<[u8]>, T: From<Bytes> + Sized>(&self, key: &K)
        -> Result<T, Self::Error>;
    async fn remove<K: AsRef<[u8]>, T: From<Bytes>>(
        &self,
        key: &K,
    ) -> Result<Option<T>, Self::Error>;
    async fn execute(&self, transaction: Self::Transaction) -> Result<(), Self::Error>;
}

#[cfg(feature = "mock")]
pub mod mock;

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
    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error>;
    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error>;
    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error>;
    async fn execute(&mut self, transaction: Self::Transaction) -> Result<(), Self::Error>;
}

#[cfg(feature = "mock")]
pub mod mock;

// std
use std::error::Error;
// crates
use async_trait::async_trait;
use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
// internal

pub trait StorageSerde {
    type Error: Error;
    fn serialize<T: Serialize>(value: T) -> Bytes;
    fn deserialize<T: DeserializeOwned>(buff: Bytes) -> Result<T, Self::Error>;
}

#[async_trait]
pub trait StorageBackend {
    type Settings: Clone + Send + Sync + 'static;
    type Error: Error + 'static;
    type Transaction: Send + Sync;
    type SerdeOperator: StorageSerde + Send + Sync + 'static;
    fn new(config: Self::Settings) -> Self;
    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error>;
    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error>;
    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error>;
    async fn execute(&mut self, transaction: Self::Transaction) -> Result<(), Self::Error>;
}

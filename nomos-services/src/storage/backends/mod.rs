#[cfg(feature = "mock")]
pub mod mock;

// std
use std::error::Error;
// crates
use async_trait::async_trait;
use bytes::Bytes;
use serde::{de::DeserializeOwned, Serialize};
// internal

/// Trait that defines how to translate from user types to the storage buffer type
pub trait StorageSerde {
    type Error: Error;
    /// Dump a type as [`Bytes`]
    fn serialize<T: Serialize>(value: T) -> Bytes;
    /// Load a type from [`Bytes`]
    fn deserialize<T: DeserializeOwned>(buff: Bytes) -> Result<T, Self::Error>;
}

/// Main storage functionality trait
#[async_trait]
pub trait StorageBackend {
    /// Backend settings
    type Settings: Clone + Send + Sync + 'static;
    /// Backend operations error type
    type Error: Error + 'static;
    /// Backend transaction type
    /// Usually it will be some function that modifies the storage directly or operates
    /// over the backend as per the backend specification.
    type Transaction: Send + Sync;
    /// Operator to dump/load custom types into the defined backend store type [`Bytes`]
    type SerdeOperator: StorageSerde + Send + Sync + 'static;
    fn new(config: Self::Settings) -> Self;
    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error>;
    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error>;
    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error>;
    /// Execute a transaction in the current backend
    async fn execute(&mut self, transaction: Self::Transaction) -> Result<(), Self::Error>;
}

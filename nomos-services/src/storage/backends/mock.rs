use std::collections::HashMap;
// std
// crates
use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;
// internal
use super::StorageBackend;

#[derive(Debug, Error)]
pub enum MockStorageError {}

pub type MockStorageTransaction = Box<dyn Fn(&mut HashMap<Bytes, Bytes>) + Send + Sync>;

pub struct MockStorage(HashMap<Bytes, Bytes>);

#[async_trait]
impl StorageBackend for MockStorage {
    type Settings = ();
    type Error = MockStorageError;
    type Transaction = MockStorageTransaction;

    fn new(_config: Self::Settings) -> Self {
        Self(HashMap::new())
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        let _ = self.0.insert(key, value);
        Ok(())
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.0.get(key).map(|b| Bytes::copy_from_slice(b)))
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.0.remove(key))
    }

    async fn execute(&mut self, transaction: Self::Transaction) -> Result<(), Self::Error> {
        transaction(&mut self.0);
        Ok(())
    }
}

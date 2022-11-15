// std
// crates
use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use thiserror::Error;
// internal
use super::StorageBackend;

#[derive(Debug, Error)]
pub enum MockStorageError {}

pub type MockStorageTransaction = Box<dyn Fn(&DashMap<Bytes, Bytes>) + Send + Sync>;

pub struct MockStorage(DashMap<Bytes, Bytes>);

#[async_trait]
impl StorageBackend for MockStorage {
    type Settings = ();
    type Error = MockStorageError;
    type Transaction = MockStorageTransaction;

    fn new(_config: Self::Settings) -> Self {
        Self(DashMap::new())
    }

    async fn store<K: AsRef<[u8]> + Send + Sync, T: Into<Bytes> + Send + Sync>(
        &self,
        key: K,
        value: T,
    ) -> Result<(), Self::Error> {
        let _ = self
            .0
            .insert(Bytes::copy_from_slice(key.as_ref()), value.into());
        Ok(())
    }

    async fn load<K: AsRef<[u8]> + Send + Sync, T: From<Bytes> + Sized>(
        &self,
        key: &K,
    ) -> Result<Option<T>, Self::Error> {
        Ok(self
            .0
            .get(key.as_ref())
            .map(|entry| T::from(Bytes::copy_from_slice(entry.value().as_ref()))))
    }

    async fn remove<K: AsRef<[u8]> + Send + Sync, T: From<Bytes>>(
        &self,
        key: &K,
    ) -> Result<Option<T>, Self::Error> {
        Ok(self.0.remove(key.as_ref()).map(|(_, v)| T::from(v)))
    }

    async fn execute(&self, transaction: Self::Transaction) -> Result<(), Self::Error> {
        transaction(&self.0);
        Ok(())
    }
}

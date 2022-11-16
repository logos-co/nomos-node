// std
use std::path::PathBuf;
// crates
use async_trait::async_trait;
use bytes::Bytes;
use sled::transaction::{ConflictableTransactionResult, TransactionError, TransactionalTree};
// internal
use super::StorageBackend;

#[derive(Clone)]
pub struct SledBackendSettings {
    db_path: PathBuf,
}

pub type SledTransaction = Box<
    dyn Fn(&TransactionalTree) -> ConflictableTransactionResult<Bytes, sled::Error> + Send + Sync,
>;

/// Sled storage backend
pub struct SledBackend(sled::Db);

#[async_trait]
impl StorageBackend for SledBackend {
    type Settings = SledBackendSettings;
    type Error = TransactionError;
    type Transaction = SledTransaction;

    fn new(config: Self::Settings) -> Self {
        Self(
            sled::open(config.db_path)
                // TODO: We should probably make initialization failable
                .map_err(|e| panic!("{e:?}"))
                .unwrap(),
        )
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        let _ = self.0.insert(key, value.to_vec())?;
        Ok(())
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.0.get(key)?.map(|ivec| ivec.to_vec().into()))
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.0.remove(key)?.map(|ivec| ivec.to_vec().into()))
    }

    async fn execute(&mut self, transaction: Self::Transaction) -> Result<(), Self::Error> {
        self.0.transaction(transaction)?;
        Ok(())
    }
}

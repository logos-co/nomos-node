// std
use std::marker::PhantomData;
use std::path::PathBuf;
// crates
use async_trait::async_trait;
use bytes::Bytes;
use sled::transaction::{
    ConflictableTransactionResult, TransactionError, TransactionResult, TransactionalTree,
};
// internal
use super::StorageBackend;
use crate::storage::backends::{StorageSerde, StorageTransaction};

/// Sled backend setting
#[derive(Clone)]
pub struct SledBackendSettings {
    /// File path to the db file
    db_path: PathBuf,
}

/// Sled transaction type
/// Function that takes a reference to the transactional tree. No `&mut` needed as sled operations
/// work over simple `&`.
pub type SledTransaction = Box<
    dyn Fn(&TransactionalTree) -> ConflictableTransactionResult<Option<Bytes>, sled::Error>
        + Send
        + Sync,
>;

impl StorageTransaction for SledTransaction {
    type Result = TransactionResult<Option<Bytes>, sled::Error>;
    type Transaction = Self;
}

/// Sled storage backend
pub struct SledBackend<SerdeOp> {
    sled: sled::Db,
    _serde_op: PhantomData<SerdeOp>,
}

#[async_trait]
impl<SerdeOp: StorageSerde + Send + Sync + 'static> StorageBackend for SledBackend<SerdeOp> {
    type Settings = SledBackendSettings;
    type Error = TransactionError;
    type Transaction = SledTransaction;
    type SerdeOperator = SerdeOp;

    fn new(config: Self::Settings) -> Self {
        Self {
            sled: sled::open(config.db_path)
                // TODO: We should probably make initialization failable
                .unwrap(),
            _serde_op: Default::default(),
        }
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        let _ = self.sled.insert(key, value.to_vec())?;
        Ok(())
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.sled.get(key)?.map(|ivec| ivec.to_vec().into()))
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.sled.remove(key)?.map(|ivec| ivec.to_vec().into()))
    }

    async fn execute(
        &mut self,
        transaction: Self::Transaction,
    ) -> Result<<Self::Transaction as StorageTransaction>::Result, Self::Error> {
        Ok(self.sled.transaction(transaction))
    }
}

#[cfg(test)]
mod test {
    use super::super::testing::NoStorageSerde;
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_store_load_remove() -> Result<(), TransactionError> {
        let temp_path = TempDir::new().unwrap();
        let sled_settings = SledBackendSettings {
            db_path: temp_path.path().to_path_buf(),
        };
        let key = "foo";
        let value = "bar";

        {
            let mut sled_db: SledBackend<NoStorageSerde> = SledBackend::new(sled_settings);
            sled_db
                .store(key.as_bytes().into(), value.as_bytes().into())
                .await?;
            let load_value = sled_db.load(key.as_bytes()).await?;
            assert_eq!(load_value, Some(value.as_bytes().into()));
            let removed_value = sled_db.remove(key.as_bytes()).await?;
            assert_eq!(removed_value, Some(value.as_bytes().into()));
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction() -> Result<(), TransactionError> {
        let temp_path = TempDir::new().unwrap();

        let sled_settings = SledBackendSettings {
            db_path: temp_path.path().to_path_buf(),
        };
        let key = "foo";
        let value = "bar";

        {
            let mut sled_db: SledBackend<NoStorageSerde> = SledBackend::new(sled_settings);
            let result = sled_db
                .execute(Box::new(move |tx| {
                    let key = key.clone();
                    let value = value.clone();
                    tx.insert(key, value)?;
                    let result = tx.get(key)?;
                    tx.remove(key)?;
                    Ok(result.map(|ivec| ivec.to_vec().into()))
                }))
                .await??;
            assert_eq!(result, Some(value.as_bytes().into()))
        }

        Ok(())
    }
}

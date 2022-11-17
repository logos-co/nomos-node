use std::marker::PhantomData;
// std
use std::path::PathBuf;
// crates
use crate::storage::backends::{StorageSerde, StorageTransaction};
use async_trait::async_trait;
use bytes::Bytes;
use sled::transaction::{
    ConflictableTransactionResult, TransactionError, TransactionResult, TransactionalTree,
};
// internal
use super::StorageBackend;

#[derive(Clone)]
pub struct SledBackendSettings {
    db_path: PathBuf,
}

pub type SledTransaction = Box<
    dyn Fn(&TransactionalTree) -> ConflictableTransactionResult<Bytes, sled::Error> + Send + Sync,
>;

impl StorageTransaction for SledTransaction {
    type Result = TransactionResult<Bytes, sled::Error>;
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

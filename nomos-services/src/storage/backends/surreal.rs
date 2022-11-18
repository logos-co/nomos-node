use std::marker::PhantomData;
// std
use std::path::{Path, PathBuf};
// crates
use async_trait::async_trait;
use bytes::Bytes;
use surrealdb::{Datastore, Error, Transaction};
// internal
use crate::storage::backends::{StorageBackend, StorageSerde, StorageTransaction};

#[derive(Clone, Debug)]
pub struct SurrealSettings {
    database_address: PathBuf,
}

pub struct SurrealBackend<SerdeOp> {
    surreal: Datastore,
    _serde_op: PhantomData<SerdeOp>,
}

pub type SurrealTransaction =
    Box<dyn Fn(Transaction) -> Result<Option<Bytes>, Error> + Send + Sync>;

impl StorageTransaction for SurrealTransaction {
    type Result = Result<Option<Bytes>, Error>;
    type Transaction = Self;
}

#[async_trait]
impl<SerdeOp: StorageSerde + Send + Sync + 'static> StorageBackend for SurrealBackend<SerdeOp> {
    type Settings = SurrealSettings;
    type Error = Error;
    type Transaction = SurrealTransaction;
    type SerdeOperator = SerdeOp;

    fn new(config: Self::Settings) -> Self {
        todo!()
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        todo!()
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        todo!()
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        todo!()
    }

    async fn execute(
        &mut self,
        transaction: Self::Transaction,
    ) -> Result<<Self::Transaction as StorageTransaction>::Result, Self::Error> {
        todo!()
    }
}

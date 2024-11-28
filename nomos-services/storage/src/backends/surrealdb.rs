use crate::backends::{StorageBackend, StorageSerde, StorageTransaction};
use bytes::Bytes;
use surrealdb::{Error, Surreal};
use tokio::task::spawn_blocking;

pub struct SurrealBackendSettings {
    pub db_path: String,
}

pub struct SurrealBackendTransaction {}

impl SurrealBackendTransaction {
    pub fn execute(&self) -> Self::Result {
        todo!()
    }
}

impl StorageTransaction for SurrealBackendTransaction {
    type Result = Result<Option<Bytes>, Error>;
    type Transaction = Self;
}

pub struct SurrealBackend<Connection: surrealdb::Connection> {
    db: Surreal<Connection>,
}

impl<Connection: surrealdb::Connection> StorageBackend for SurrealBackend<Connection> {
    type Settings = SurrealBackendSettings;
    type Error = Error;
    type Transaction = SurrealBackendTransaction;
    type SerdeOperator = ();

    fn new(config: Self::Settings) -> Result<Self, Self::Error> {
        let Self::Settings { db_path } = config;
        let db = Surreal::new(db_path);
        spawn_blocking(async {
            db.connect().await?;
            Ok(Self { db })
        })
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        let key = String::from(key);
        let _record = self.db.create(("kv", key)).content(value).await?;
        Ok(())
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        let key = String::from(key);
        let record = self.db.select(("kv", key)).await?;
        Ok(record)
    }

    async fn load_prefix(&mut self, prefix: &[u8]) -> Result<Vec<Bytes>, Self::Error> {
        let prefix = String::from(prefix);
        let query =
            format!("SELECT * FROM kv WHERE string::starts_with(record::id(id), \"{prefix}\")");
        let mut response = self.db.query(query).await?;
        let records = response.stream(()).map(|record| record.content()).collect();
        Ok(records)
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        Ok(self.db.delete(("kv", key)).await?)
    }

    async fn execute(
        &mut self,
        transaction: Self::Transaction,
    ) -> Result<<Self::Transaction as StorageTransaction>::Result, Self::Error> {
        todo!()
    }
}

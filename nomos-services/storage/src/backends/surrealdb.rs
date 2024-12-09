use std::marker::PhantomData;
use std::str::from_utf8;

use async_trait::async_trait;
use bytes::Bytes;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{Deserialize, Serialize};
use surrealdb::engine::any::{connect, Any};
use surrealdb::Surreal;
use tokio::task::block_in_place;

use crate::backends::{StorageBackend, StorageSerde, StorageTransaction};

#[derive(Debug, Serialize, Deserialize)]
struct Data {
    pub data: Bytes,
}

impl Data {
    fn new(value: Bytes) -> Self {
        Self { data: value }
    }

    fn to_bytes(&self) -> Bytes {
        self.data.clone()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Surreal(#[from] surrealdb::Error),
    #[error(transparent)]
    Utf8(#[from] std::str::Utf8Error),
}

#[derive(Clone)]
pub struct SurrealBackendSettings {
    pub db_path: String,
    pub namespace: String,
    pub database: String,
    pub overwatch_handle: OverwatchHandle,
}

impl SurrealBackendSettings {
    pub fn new(
        db_path: impl Into<String>,
        namespace: impl Into<String>,
        database: impl Into<String>,
        overwatch_handle: OverwatchHandle,
    ) -> Self {
        Self {
            db_path: db_path.into(),
            namespace: namespace.into(),
            database: database.into(),
            overwatch_handle,
        }
    }
}

pub struct SurrealBackendTransaction {}

impl StorageTransaction for SurrealBackendTransaction {
    type Result = Result<Option<Bytes>, Error>;
    type Transaction = Self;
}

pub struct SurrealBackend<SerdeOp>
where
    SerdeOp: StorageSerde + Send + Sync + 'static,
{
    db: Surreal<Any>,
    _serde_op: PhantomData<SerdeOp>,
}

#[async_trait]
impl<SerdeOp> StorageBackend for SurrealBackend<SerdeOp>
where
    SerdeOp: StorageSerde + Send + Sync + 'static,
{
    type Settings = SurrealBackendSettings;
    type Error = Error;
    type Transaction = SurrealBackendTransaction;
    type SerdeOperator = SerdeOp;

    fn new(config: Self::Settings) -> Result<Self, Self::Error> {
        let Self::Settings {
            db_path,
            namespace,
            database,
            overwatch_handle,
        } = config;

        let connect_to_surrealdb = async {
            let db = connect(db_path).await?;
            db.use_ns(namespace).use_db(database).await?;
            Ok(Self {
                db,
                _serde_op: Default::default(),
            })
        };

        block_in_place(|| overwatch_handle.runtime().block_on(connect_to_surrealdb))
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        let key = from_utf8(&*key)?;
        let data = Data::new(value);
        let _record: Option<Data> = self.db.create(("kv", key)).content(data).await?;
        Ok(())
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        let key = from_utf8(key)?;
        let record: Option<Data> = self.db.select(("kv", key)).await?;
        Ok(record.as_ref().map(Data::to_bytes))
    }

    async fn load_prefix(&mut self, prefix: &[u8]) -> Result<Vec<Bytes>, Self::Error> {
        let prefix = from_utf8(prefix)?;
        let query =
            format!("SELECT * FROM kv WHERE string::starts_with(record::id(id), \"{prefix}\")");
        let mut response = self.db.query(query).await?;
        let records: Vec<Data> = response.take(0)?;
        let records = records.iter().map(Data::to_bytes).collect();
        Ok(records)
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        let key = from_utf8(key)?;
        let record: Option<Data> = self.db.delete(("kv", key)).await?;
        Ok(record.as_ref().map(Data::to_bytes))
    }

    async fn execute(
        &mut self,
        _transaction: Self::Transaction,
    ) -> Result<<Self::Transaction as StorageTransaction>::Result, Self::Error> {
        unimplemented!("SurrealDB does not support transactions yet.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::testing::NoStorageSerde;
    use tokio::runtime::Handle;

    fn get_test_config() -> SurrealBackendSettings {
        let (sender, _) = tokio::sync::mpsc::channel(1);
        SurrealBackendSettings::new(
            String::from("mem://"),
            String::from("namespace"),
            String::from("database"),
            OverwatchHandle::new(Handle::current(), sender),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_load_remove() -> Result<(), Error> {
        let key = Bytes::from("my-key");
        let value = Bytes::from("my-value");

        let mut backend = SurrealBackend::<NoStorageSerde>::new(get_test_config())?;

        let store_result = backend.store(key.clone(), value.clone()).await;
        assert!(store_result.is_ok(), "{:?}", store_result.err());
        assert_eq!(store_result?, ());

        let load_result = backend.load(&key).await;
        assert!(load_result.is_ok(), "{:?}", load_result);
        assert_eq!(load_result?, Some(value.clone()));

        let remove_result = backend.remove(&key).await;
        assert!(remove_result.is_ok(), "{:?}", remove_result);
        assert_eq!(remove_result?, Some(value.clone()));

        let load_result = backend.load(&key).await;
        assert!(load_result.is_ok(), "{:?}", load_result);
        assert_eq!(load_result?, None);

        Ok(())
    }
}

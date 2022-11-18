use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
// std
// crates
use async_trait::async_trait;
use bytes::Bytes;
use surrealdb::{Datastore, Error, Transaction};
// internal
use crate::storage::backends::{StorageBackend, StorageSerde, StorageTransaction};

/// SurrealDb backend settings
#[derive(Clone, Debug)]
pub struct SurrealSettings {
    /// Connection address
    database_address: String,
}

/// SurrealDb backend wrapper
pub struct SurrealBackend<SerdeOp> {
    surreal: Datastore,
    _serde_op: PhantomData<SerdeOp>,
}

/// SurrealDb transaction
/// An asynchronous executor that operates over a surreal [`Transaction`]
pub type SurrealTransaction = Box<
    dyn Fn(Transaction) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, Error>> + Send + Sync>>
        + Send
        + Sync,
>;

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
        // TODO: should we changes `new` to async?
        let surreal =
            futures::executor::block_on(Datastore::new(config.database_address.as_str())).unwrap();
        Self {
            surreal,
            _serde_op: Default::default(),
        }
    }

    async fn store(&mut self, key: Bytes, value: Bytes) -> Result<(), Self::Error> {
        let mut tx = self.surreal.transaction(true, false).await?;
        let _ = tx.set(key, value).await?;
        tx.commit().await
    }

    async fn load(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        let mut tx = self.surreal.transaction(true, false).await?;
        let result = tx.get(key).await?;
        let _ = tx.commit().await?;
        Ok(result.map(Bytes::from))
    }

    async fn remove(&mut self, key: &[u8]) -> Result<Option<Bytes>, Self::Error> {
        let mut tx = self.surreal.transaction(true, false).await?;
        let value = tx.get(key).await?;
        let _ = tx.del(key).await?;
        let _ = tx.commit().await;
        Ok(value.map(Bytes::from))
    }

    async fn execute(
        &mut self,
        transaction: Self::Transaction,
    ) -> Result<<Self::Transaction as StorageTransaction>::Result, Self::Error> {
        let tx = self.surreal.transaction(true, false).await?;
        Ok(transaction(tx).await)
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::NoStorageSerde;
    use super::*;

    #[tokio::test]
    async fn test_store_load_remove() -> Result<(), Error> {
        let config: SurrealSettings = SurrealSettings {
            database_address: "memory".to_string(),
        };

        let mut surreal_db: SurrealBackend<NoStorageSerde> = SurrealBackend::new(config);
        let key = "foo";
        let value = "bar";
        let _ = StorageBackend::store(
            &mut surreal_db,
            key.as_bytes().into(),
            value.as_bytes().into(),
        )
        .await?;

        let load_value = StorageBackend::load(&mut surreal_db, key.as_bytes()).await?;
        assert_eq!(load_value, Some(value.as_bytes().into()));

        let removed_value = StorageBackend::remove(&mut surreal_db, key.as_bytes()).await?;
        assert_eq!(removed_value, Some(value.as_bytes().into()));

        let load_value = StorageBackend::load(&mut surreal_db, key.as_bytes()).await?;
        assert_eq!(load_value, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction() -> Result<(), Error> {
        let config: SurrealSettings = SurrealSettings {
            database_address: "memory".to_string(),
        };

        let mut surreal_db: SurrealBackend<NoStorageSerde> = SurrealBackend::new(config);
        let key = "foo";
        let value = "bar";

        let result = StorageBackend::execute(
            &mut surreal_db,
            Box::new(move |mut tx| {
                Box::pin(async move {
                    tx.set(key.clone(), value).await?;
                    let value = tx.get(key.clone()).await?;
                    tx.del(key.clone()).await?;
                    Ok(value.map(Bytes::from))
                })
            }),
        )
        .await??;
        assert_eq!(result, Some(value.as_bytes().into()));
        Ok(())
    }
}

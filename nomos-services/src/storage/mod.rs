pub mod backends;

// std
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
// crates
use async_trait::async_trait;
use bytes::Bytes;
use overwatch::services::handle::ServiceStateHandle;
use serde::de::DeserializeOwned;
// internal
use crate::storage::backends::StorageSerde;
use backends::StorageBackend;
use overwatch::services::relay::RelayMessage;
use overwatch::services::state::{NoOperator, NoState};
use overwatch::services::{ServiceCore, ServiceData, ServiceId};
use serde::Serialize;

/// Storage message that maps to [`StorageBackend`] trait
pub enum StorageMsg<Backend: StorageBackend> {
    Load {
        key: Bytes,
        reply_channel: tokio::sync::oneshot::Sender<Option<Bytes>>,
    },
    Store {
        key: Bytes,
        value: Bytes,
    },
    Remove {
        key: Bytes,
        reply_channel: tokio::sync::oneshot::Sender<Option<Bytes>>,
    },
    Execute {
        transaction: Backend::Transaction,
    },
}

/// Reply channel for storage messages
pub struct StorageReplyReceiver<T, Backend> {
    channel: tokio::sync::oneshot::Receiver<T>,
    _backend: PhantomData<Backend>,
}

impl<T, Backend> StorageReplyReceiver<T, Backend> {
    pub fn new(channel: tokio::sync::oneshot::Receiver<T>) -> Self {
        Self {
            channel,
            _backend: Default::default(),
        }
    }

    pub fn into_inner(self) -> tokio::sync::oneshot::Receiver<T> {
        self.channel
    }
}

impl<Backend: StorageBackend> StorageReplyReceiver<Bytes, Backend> {
    /// Receive and transform the reply into the desired type
    /// Target type must implement `From` from the original backend stored type.
    pub async fn recv<Output>(self) -> Result<Output, tokio::sync::oneshot::error::RecvError>
    where
        Output: DeserializeOwned,
    {
        self.channel
            .await
            // TODO: This should probably just return a result anyway. But for now we can consider in infallible.
            .map(|b| {
                Backend::SerdeOperator::deserialize(b)
                    .expect("Recovery from storage should never fail")
            })
    }
}

impl<Backend: StorageBackend> StorageMsg<Backend> {
    pub fn new_load_message<K: Serialize>(
        key: K,
    ) -> (
        StorageMsg<Backend>,
        StorageReplyReceiver<Option<Bytes>, Backend>,
    ) {
        let key = Backend::SerdeOperator::serialize(key);
        let (reply_channel, receiver) = tokio::sync::oneshot::channel();
        (
            Self::Load { key, reply_channel },
            StorageReplyReceiver::new(receiver),
        )
    }

    pub fn new_store_message<K: Serialize, V: Serialize>(key: K, value: V) -> StorageMsg<Backend> {
        let key = Backend::SerdeOperator::serialize(key);
        let value = Backend::SerdeOperator::serialize(value);
        StorageMsg::Store { key, value }
    }

    pub fn new_remove_message<K: Serialize>(
        key: K,
    ) -> (
        StorageMsg<Backend>,
        StorageReplyReceiver<Option<Bytes>, Backend>,
    ) {
        let key = Backend::SerdeOperator::serialize(key);
        let (reply_channel, receiver) = tokio::sync::oneshot::channel();
        (
            Self::Remove { key, reply_channel },
            StorageReplyReceiver::new(receiver),
        )
    }

    pub fn new_transaction_message(transaction: Backend::Transaction) -> StorageMsg<Backend> {
        Self::Execute { transaction }
    }
}

// Implement `Debug` manually to avoid constraining `Backend` to `Debug`
impl<Backend: StorageBackend> Debug for StorageMsg<Backend> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageMsg::Load { key, .. } => {
                write!(f, "Load {{ {key:?} }}")
            }
            StorageMsg::Store { key, value } => {
                write!(f, "Store {{ {key:?}, {value:?}}}")
            }
            StorageMsg::Remove { key, .. } => {
                write!(f, "Remove {{ {key:?} }}")
            }
            StorageMsg::Execute { .. } => write!(f, "Execute transaction"),
        }
    }
}

impl<Backend: StorageBackend + 'static> RelayMessage for StorageMsg<Backend> {}

/// Storage error
/// Errors that may happen when performing storage operations
#[derive(Debug, thiserror::Error)]
enum StorageServiceError<Backend: StorageBackend> {
    #[error("Couldn't send a reply")]
    ReplyError(Option<Bytes>),
    #[error("Storage backend error")]
    BackendError(#[source] Backend::Error),
}

/// Storage service that wraps a [`StorageBackend`]
pub struct StorageService<Backend: StorageBackend + Send + Sync + 'static> {
    backend: Backend,
    service_state: ServiceStateHandle<Self>,
}

impl<Backend: StorageBackend + Send + Sync + 'static> StorageService<Backend> {
    /// Handle load message
    async fn handle_load(
        backend: &mut Backend,
        key: Bytes,
        reply_channel: tokio::sync::oneshot::Sender<Option<Bytes>>,
    ) -> Result<(), StorageServiceError<Backend>> {
        let result: Option<Bytes> = backend
            .load(&key)
            .await
            .map_err(StorageServiceError::BackendError)?;
        reply_channel
            .send(result)
            .map_err(StorageServiceError::ReplyError)
    }

    /// Handle remove message
    async fn handle_remove(
        backend: &mut Backend,
        key: Bytes,
        reply_channel: tokio::sync::oneshot::Sender<Option<Bytes>>,
    ) -> Result<(), StorageServiceError<Backend>> {
        let result: Option<Bytes> = backend
            .remove(&key)
            .await
            .map_err(StorageServiceError::BackendError)?;
        reply_channel
            .send(result)
            .map_err(StorageServiceError::ReplyError)
    }

    /// Handle store message
    async fn handle_store(
        backend: &mut Backend,
        key: Bytes,
        value: Bytes,
    ) -> Result<(), StorageServiceError<Backend>> {
        backend
            .store(key, value)
            .await
            .map_err(StorageServiceError::BackendError)
    }

    /// Handle execute message
    async fn handle_execute(
        backend: &mut Backend,
        transaction: Backend::Transaction,
    ) -> Result<(), StorageServiceError<Backend>> {
        backend
            .execute(transaction)
            .await
            .map_err(StorageServiceError::BackendError)
    }
}

#[async_trait]
impl<Backend: StorageBackend + Send + Sync + 'static> ServiceCore for StorageService<Backend> {
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
        Self {
            backend: Backend::new(service_state.settings_reader.get_updated_settings()),
            service_state,
        }
    }

    async fn run(mut self) {
        let Self {
            mut backend,
            service_state: ServiceStateHandle {
                mut inbound_relay, ..
            },
        } = self;
        let backend = &mut backend;
        while let Some(msg) = inbound_relay.recv().await {
            if let Err(e) = match msg {
                StorageMsg::Load { key, reply_channel } => {
                    Self::handle_load(backend, key, reply_channel).await
                }
                StorageMsg::Store { key, value } => Self::handle_store(backend, key, value).await,
                StorageMsg::Remove { key, reply_channel } => {
                    Self::handle_remove(backend, key, reply_channel).await
                }
                StorageMsg::Execute { transaction } => {
                    Self::handle_execute(backend, transaction).await
                }
            } {
                // TODO: add proper logging
                println!("{e}");
            }
        }
    }
}

impl<Backend: StorageBackend + Send + Sync> ServiceData for StorageService<Backend> {
    const SERVICE_ID: ServiceId = "Storage";
    type Settings = Backend::Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = StorageMsg<Backend>;
}

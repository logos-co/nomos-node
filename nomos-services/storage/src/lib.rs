pub mod backends;

// std
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
// crates
use async_trait::async_trait;
use bytes::Bytes;
use overwatch_rs::services::handle::ServiceStateHandle;
use serde::de::DeserializeOwned;
use serde::Serialize;
// internal
use backends::StorageBackend;
use backends::{StorageSerde, StorageTransaction};
use overwatch_rs::services::relay::RelayMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};

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
        reply_channel:
            tokio::sync::oneshot::Sender<<Backend::Transaction as StorageTransaction>::Result>,
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

impl<Backend: StorageBackend> StorageReplyReceiver<Option<Bytes>, Backend> {
    /// Receive and transform the reply into the desired type
    /// Target type must implement `From` from the original backend stored type.
    pub async fn recv<Output>(
        self,
    ) -> Result<Option<Output>, tokio::sync::oneshot::error::RecvError>
    where
        Output: DeserializeOwned,
    {
        self.channel
            .await
            // TODO: This should probably just return a result anyway. But for now we can consider in infallible.
            .map(|maybe_bytes| {
                maybe_bytes.map(|bytes| {
                    Backend::SerdeOperator::deserialize(bytes)
                        .expect("Recovery from storage should never fail")
                })
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

    pub fn new_transaction_message(
        transaction: Backend::Transaction,
    ) -> (
        StorageMsg<Backend>,
        StorageReplyReceiver<<Backend::Transaction as StorageTransaction>::Result, Backend>,
    ) {
        let (reply_channel, receiver) = tokio::sync::oneshot::channel();
        (
            Self::Execute {
                transaction,
                reply_channel,
            },
            StorageReplyReceiver::new(receiver),
        )
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
    #[error("Couldn't send a reply for operation `{operation}` with key [{key:?}]")]
    ReplyError { operation: String, key: Bytes },
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
            .map_err(|_| StorageServiceError::ReplyError {
                operation: "Load".to_string(),
                key,
            })
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
            .map_err(|_| StorageServiceError::ReplyError {
                operation: "Remove".to_string(),
                key,
            })
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
        reply_channel: tokio::sync::oneshot::Sender<
            <Backend::Transaction as StorageTransaction>::Result,
        >,
    ) -> Result<(), StorageServiceError<Backend>> {
        let result = backend
            .execute(transaction)
            .await
            .map_err(StorageServiceError::BackendError)?;
        reply_channel
            .send(result)
            .map_err(|_| StorageServiceError::ReplyError {
                operation: "Execute".to_string(),
                key: Bytes::new(),
            })
    }
}

#[async_trait]
impl<Backend: StorageBackend + Send + Sync + 'static> ServiceCore for StorageService<Backend> {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self {
            backend: Backend::new(service_state.settings_reader.get_updated_settings())?,
            service_state,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
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
                StorageMsg::Execute {
                    transaction,
                    reply_channel,
                } => Self::handle_execute(backend, transaction, reply_channel).await,
            } {
                // TODO: add proper logging
                println!("{e}");
            }
        }
        Ok(())
    }
}

impl<Backend: StorageBackend + Send + Sync> ServiceData for StorageService<Backend> {
    const SERVICE_ID: ServiceId = "Storage";
    type Settings = Backend::Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = StorageMsg<Backend>;
}

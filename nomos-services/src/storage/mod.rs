pub mod backends;

// std
use std::fmt::{Debug, Formatter};
// crates
use async_trait::async_trait;
use bytes::Bytes;
use overwatch::services::handle::ServiceStateHandle;
// internal
use backends::StorageBackend;
use overwatch::services::relay::RelayMessage;
use overwatch::services::state::{NoOperator, NoState};
use overwatch::services::{ServiceCore, ServiceData, ServiceId};

pub enum StorageMsg<Backend: StorageBackend> {
    Load {
        key: Bytes,
        reply_channel: tokio::sync::oneshot::Sender<Bytes>,
    },
    Store {
        key: Bytes,
        value: Bytes,
    },
    Remove {
        key: Bytes,
        reply_channel: Option<tokio::sync::oneshot::Sender<Bytes>>,
    },
    Execute {
        transaction: Backend::Transaction,
    },
}

//
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

#[derive(Debug, thiserror::Error)]
enum StorageServiceError<Backend: StorageBackend> {
    #[error("Couldn't send a reply")]
    ReplyError(Bytes),
    #[error("Storage backend error")]
    BackendError(#[source] Backend::Error),
}

pub struct StorageService<Backend: StorageBackend + Send + Sync + 'static> {
    backend: Backend,
    service_state: ServiceStateHandle<Self>,
}

impl<Backend: StorageBackend + Send + Sync + 'static> StorageService<Backend> {
    async fn handle_load(
        backend: &Backend,
        key: Bytes,
        reply_channel: tokio::sync::oneshot::Sender<Bytes>,
    ) -> Result<(), StorageServiceError<Backend>> {
        let result: Bytes = backend
            .load(&key)
            .await
            .map_err(StorageServiceError::BackendError)?;
        reply_channel
            .send(result)
            .map_err(StorageServiceError::ReplyError)
    }

    async fn handle_remove(
        backend: &Backend,
        key: Bytes,
        reply_channel: Option<tokio::sync::oneshot::Sender<Bytes>>,
    ) -> Result<(), StorageServiceError<Backend>> {
        if let Some((result, reply_channel)) = backend
            .remove(&key)
            .await
            .map_err(StorageServiceError::BackendError)?
            .zip(reply_channel)
        {
            reply_channel
                .send(result)
                .map_err(StorageServiceError::ReplyError)?;
        }

        Ok(())
    }

    async fn handle_store(
        backend: &Backend,
        key: Bytes,
        value: Bytes,
    ) -> Result<(), StorageServiceError<Backend>> {
        backend
            .store(&key, value)
            .await
            .map_err(StorageServiceError::BackendError)
    }

    async fn handle_execute(
        backend: &Backend,
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

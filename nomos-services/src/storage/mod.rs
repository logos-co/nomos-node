pub mod backends;

// std

use async_trait::async_trait;
use std::fmt::{Debug, Formatter};
// crates
use bytes::Bytes;
use overwatch::services::handle::ServiceStateHandle;
// internal
use backends::StorageBackend;
use overwatch::services::relay::RelayMessage;
use overwatch::services::state::{NoOperator, NoState};
use overwatch::services::{ServiceCore, ServiceData, ServiceId};

pub enum StorageMsg<Backend: StorageBackend> {
    Load {
        key: Box<[u8]>,
        reply_channel: tokio::sync::oneshot::Sender<Bytes>,
    },
    Store {
        key: Box<[u8]>,
        value: Bytes,
    },
    Remove {
        key: Box<[u8]>,
    },
    Execute {
        transaction: Backend::Transaction,
    },
}

impl<Backend: StorageBackend> Debug for StorageMsg<Backend> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl<Backend: StorageBackend + 'static> RelayMessage for StorageMsg<Backend> {}

pub struct StorageService<Backend: StorageBackend + Send + Sync + 'static> {
    backend: Backend,
    service_state: ServiceStateHandle<Self>,
}

#[async_trait]
impl<Backend: StorageBackend + Send + Sync + 'static> ServiceCore for StorageService<Backend> {
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
        Self {
            backend: Backend::new(service_state.settings_reader.get_updated_settings()),
            service_state,
        }
    }

    async fn run(self) {
        todo!()
    }
}

impl<Backend: StorageBackend + Send + Sync> ServiceData for StorageService<Backend> {
    const SERVICE_ID: ServiceId = "Storage";
    type Settings = Backend::Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = StorageMsg<Backend>;
}

pub mod backend;

use std::fmt::{Debug, Formatter};
use std::sync::mpsc::Sender;

use backend::DaStorageBackend;
use futures::StreamExt;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::RelayMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use serde::{Deserialize, Serialize};
use tracing::error;

pub struct DataAvailabilityService<Backend>
where
    Backend: DaStorageBackend,
    Backend::Blob: 'static,
    Backend::VID: 'static,
{
    service_state: ServiceStateHandle<Self>,
    backend: Backend,
}

pub enum DaMsg<B, V> {
    // Add verified blob to the da storage.
    Add {
        blob: B,
    },
    // Promote blob to indexed application data.
    //
    // TODO: naming - Store is used by the verifier and the indexer services. Verifier adds
    // verified blobs, indexer tracks the blockchain and promotes blobs to be available via the
    // api.
    Promote {
        vid: V,
    },
    Get {
        ids: Box<dyn Iterator<Item = B> + Send>,
        reply_channel: Sender<Vec<B>>,
    },
}

impl<B: 'static, V: 'static> Debug for DaMsg<B, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaMsg::Add { .. } => {
                write!(f, "DaMsg::Add")
            }
            DaMsg::Promote { .. } => {
                write!(f, "DaMsg::Promote")
            }
            DaMsg::Get { .. } => {
                write!(f, "DaMsg::Get")
            }
        }
    }
}

impl<B: 'static, V: 'static> RelayMessage for DaMsg<B, V> {}

impl<Backend> ServiceData for DataAvailabilityService<Backend>
where
    Backend: DaStorageBackend,
    Backend::Blob: 'static,
    Backend::VID: 'static,
{
    const SERVICE_ID: ServiceId = "DaStorage";
    type Settings = Settings<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaMsg<Backend::Blob, Backend::VID>;
}

impl<Backend> DataAvailabilityService<Backend>
where
    Backend: DaStorageBackend + Send + Sync,
    Backend::Blob: 'static,
    Backend::VID: 'static,
{
    async fn handle_da_msg(
        backend: &Backend,
        msg: DaMsg<Backend::Blob, Backend::VID>,
    ) -> Result<(), DynError> {
        match msg {
            DaMsg::Add { blob } => {
                todo!()
            }
            DaMsg::Promote { vid } => {
                todo!()
            }
            DaMsg::Get { ids, reply_channel } => {
                todo!()
            }
        }
        Ok(())
    }

    async fn should_stop_service(message: LifecycleMessage) -> bool {
        match message {
            LifecycleMessage::Shutdown(sender) => {
                if sender.send(()).is_err() {
                    error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
            LifecycleMessage::Kill => true,
        }
    }
}

#[async_trait::async_trait]
impl<Backend> ServiceCore for DataAvailabilityService<Backend>
where
    Backend: DaStorageBackend + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Backend::Blob: Debug + Send + Sync,
    Backend::VID: Debug + Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let backend = Backend::new(settings.backend);
        Ok(Self {
            service_state,
            backend,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            backend,
        } = self;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    todo!()
                }
                Some(msg) = lifecycle_stream.next() => {
                    todo!()
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<B> {
    pub backend: B,
}

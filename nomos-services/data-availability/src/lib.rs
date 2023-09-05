mod backend;
mod network;

// std
use overwatch_rs::DynError;
use std::fmt::{Debug, Formatter};
// crates
use tokio::sync::oneshot::Sender;
// internal
use crate::backend::DaBackend;
use crate::network::NetworkAdapter;
use nomos_network::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};

pub struct DataAvailabilityService<B, N>
where
    B: DaBackend,
    B::Blob: 'static,
    N: NetworkAdapter<Blob = B::Blob>,
{
    service_state: ServiceStateHandle<Self>,
    backend: B,
    network_relay: Relay<NetworkService<N::Backend>>,
}

pub enum DaMsg<Blob> {
    PendingBlobs {
        reply_channel: Sender<Box<dyn Iterator<Item = Blob> + Send>>,
    },
}

impl<Blob: 'static> Debug for DaMsg<Blob> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaMsg::PendingBlobs { .. } => {
                write!(f, "DaMsg::PendingBlobs")
            }
        }
    }
}

impl<Blob: 'static> RelayMessage for DaMsg<Blob> {}

impl<B, N> ServiceData for DataAvailabilityService<B, N>
where
    B: DaBackend,
    B::Blob: 'static,
    N: NetworkAdapter<Blob = B::Blob>,
{
    const SERVICE_ID: ServiceId = "DA";
    type Settings = B::Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaMsg<B::Blob>;
}

#[async_trait::async_trait]
impl<B, N> ServiceCore for DataAvailabilityService<B, N>
where
    B: DaBackend + Send,
    B::Settings: Clone + Send + Sync + 'static,
    N: NetworkAdapter<Blob = B::Blob> + Send,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let backend_settings = service_state.settings_reader.get_updated_settings();
        let backend = B::new(backend_settings);
        Ok(Self {
            service_state,
            backend,
            network_relay,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        Ok(())
    }
}

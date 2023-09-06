mod backend;
mod network;

// std
use overwatch_rs::DynError;
use std::fmt::{Debug, Formatter};
// crates
use futures::StreamExt;
use tokio::sync::oneshot::Sender;
// internal
use crate::backend::{DaBackend, DaError};
use crate::network::NetworkAdapter;
use nomos_network::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use tokio::sync::oneshot;

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
    B::Blob: Send,
    N: NetworkAdapter<Blob = B::Blob> + Send + Sync,
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
        let Self {
            mut service_state,
            mut backend,
            network_relay,
        } = self;

        let network_relay = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let adapter = N::new(network_relay).await;
        let mut network_blobs = adapter.blob_stream().await;
        loop {
            tokio::select! {
                Some((blob, reply_channel)) = network_blobs.next() => {
                    if let Err(e) = handle_new_blob(&mut backend, blob, reply_channel).await {
                        tracing::debug!("Failed to add a new received blob: {e:?}");
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    if let Err(e) = handle_da_msg(&mut backend, msg).await {
                        tracing::debug!("Failed to handle da msg: {e:?}");
                    }
                }
            }
        }
    }
}

async fn handle_new_blob<B: DaBackend, Reply>(
    backend: &mut B,
    blob: B::Blob,
    _reply_channel: oneshot::Sender<Reply>,
) -> Result<(), DaError> {
    // we need to handle the reply (verification + signature)
    backend.add_blob(blob)
}

async fn handle_da_msg<B: DaBackend>(backend: &mut B, msg: DaMsg<B::Blob>) -> Result<(), DaError> {
    match msg {
        DaMsg::PendingBlobs { reply_channel } => {
            let pending_blobs = backend.pending_blobs();
            if reply_channel.send(pending_blobs).is_err() {
                tracing::debug!("Could not send pending blobs");
            }
        }
    }
    Ok(())
}

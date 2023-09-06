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
use nomos_core::blob::Blob;
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

pub enum DaMsg<B: Blob> {
    PendingBlobs {
        reply_channel: Sender<Box<dyn Iterator<Item = B> + Send>>,
    },
    RemoveBlobs {
        blobs: Box<dyn Iterator<Item = <B as Blob>::Hash> + Send>,
    },
}

impl<B: Blob + 'static> Debug for DaMsg<B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaMsg::PendingBlobs { .. } => {
                write!(f, "DaMsg::PendingBlobs")
            }
            DaMsg::RemoveBlobs { .. } => {
                write!(f, "DaMsg::RemoveBlobs")
            }
        }
    }
}

impl<B: Blob + 'static> RelayMessage for DaMsg<B> {}

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
    B: DaBackend + Send + Sync,
    B::Settings: Clone + Send + Sync + 'static,
    B::Blob: Send,
    <B::Blob as Blob>::Hash: Debug + Send + Sync,
    // TODO: Reply type must be piped together, for now empty array.
    N: NetworkAdapter<Blob = B::Blob, Attestation = [u8; 32]> + Send + Sync,
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
                Some(blob) = network_blobs.next() => {
                    if let Err(e) = handle_new_blob(&mut backend, &adapter, blob).await {
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

async fn handle_new_blob<
    B: DaBackend,
    A: NetworkAdapter<Blob = B::Blob, Attestation = [u8; 32]>,
>(
    backend: &mut B,
    adapter: &A,
    blob: B::Blob,
) -> Result<(), DaError> {
    // we need to handle the reply (verification + signature)
    backend.add_blob(blob).await?;
    adapter
        .send_attestation([0u8; 32])
        .await
        .map_err(DaError::Dyn)
}

async fn handle_da_msg<B: DaBackend>(backend: &mut B, msg: DaMsg<B::Blob>) -> Result<(), DaError>
where
    <B::Blob as Blob>::Hash: Debug,
{
    match msg {
        DaMsg::PendingBlobs { reply_channel } => {
            let pending_blobs = backend.pending_blobs().await;
            if reply_channel.send(pending_blobs).is_err() {
                tracing::debug!("Could not send pending blobs");
            }
        }
        DaMsg::RemoveBlobs { blobs } => {
            let backend = &*backend;
            futures::stream::iter(blobs)
                .for_each_concurrent(None, |blob| async move {
                    if let Err(e) = backend.remove_blob(&blob).await {
                        tracing::debug!("Could not remove blob {blob:?} due to: {e:?}");
                    }
                })
                .await;
        }
    }
    Ok(())
}

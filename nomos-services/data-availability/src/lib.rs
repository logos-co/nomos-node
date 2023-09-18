pub mod backend;
pub mod network;

// std
use overwatch_rs::DynError;
use std::fmt::{Debug, Formatter};
// crates
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::Sender;
// internal
use crate::backend::{DaBackend, DaError};
use crate::network::NetworkAdapter;
use nomos_core::da::{blob::Blob, DaProtocol};
use nomos_network::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};

pub struct DataAvailabilityService<Protocol, Backend, Network>
where
    Protocol: DaProtocol,
    Backend: DaBackend<Blob = Protocol::Blob>,
    Backend::Blob: 'static,
    Network: NetworkAdapter<Blob = Protocol::Blob, Attestation = Protocol::Attestation>,
{
    service_state: ServiceStateHandle<Self>,
    backend: Backend,
    da: Protocol,
    network_relay: Relay<NetworkService<Network::Backend>>,
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

impl<Protocol, Backend, Network> ServiceData for DataAvailabilityService<Protocol, Backend, Network>
where
    Protocol: DaProtocol,
    Backend: DaBackend<Blob = Protocol::Blob>,
    Backend::Blob: 'static,
    Network: NetworkAdapter<Blob = Protocol::Blob, Attestation = Protocol::Attestation>,
{
    const SERVICE_ID: ServiceId = "DA";
    type Settings = Settings<Protocol::Settings, Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaMsg<Protocol::Blob>;
}

#[async_trait::async_trait]
impl<Protocol, Backend, Network> ServiceCore for DataAvailabilityService<Protocol, Backend, Network>
where
    Protocol: DaProtocol + Send + Sync,
    Backend: DaBackend<Blob = Protocol::Blob> + Send + Sync,
    Protocol::Settings: Clone + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Protocol::Blob: Send,
    Protocol::Attestation: Send,
    <Backend::Blob as Blob>::Hash: Debug + Send + Sync,
    Network:
        NetworkAdapter<Blob = Protocol::Blob, Attestation = Protocol::Attestation> + Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let settings = service_state.settings_reader.get_updated_settings();
        let backend = Backend::new(settings.backend);
        let da = Protocol::new(settings.da_protocol);
        Ok(Self {
            service_state,
            backend,
            da,
            network_relay,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            mut backend,
            mut da,
            network_relay,
        } = self;

        let network_relay = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let adapter = Network::new(network_relay).await;
        let mut network_blobs = adapter.blob_stream().await;
        loop {
            tokio::select! {
                Some(blob) = network_blobs.next() => {
                    if let Err(e) = handle_new_blob(&mut da, &mut backend, &adapter, blob).await {
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
    Protocol: DaProtocol,
    Backend: DaBackend<Blob = Protocol::Blob>,
    A: NetworkAdapter<Blob = Protocol::Blob, Attestation = Protocol::Attestation>,
>(
    da: &mut Protocol,
    backend: &mut Backend,
    adapter: &A,
    blob: Protocol::Blob,
) -> Result<(), DaError> {
    // we need to handle the reply (verification + signature)
    let attestation = da.attest(&blob);
    backend.add_blob(blob).await?;
    // we do not call `da.recv_blob` here because that is meant to
    // be called to retrieve the original data, while here we're only interested
    // in storing the blob.
    // We might want to refactor the backend to be part of implementations of the
    // Da protocol instead of this service and clear this confusion.
    adapter
        .send_attestation(attestation)
        .await
        .map_err(DaError::Dyn)
}

async fn handle_da_msg<B: DaBackend>(backend: &mut B, msg: DaMsg<B::Blob>) -> Result<(), DaError>
where
    <B::Blob as Blob>::Hash: Debug,
{
    match msg {
        DaMsg::PendingBlobs { reply_channel } => {
            let pending_blobs = backend.pending_blobs();
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<P, B> {
    pub da_protocol: P,
    pub backend: B,
}

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
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use tracing::error;

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
    RemoveBlobs {
        blobs: Box<dyn Iterator<Item = <B as Blob>::Hash> + Send>,
    },
    Get {
        ids: Box<dyn Iterator<Item = <B as Blob>::Hash> + Send>,
        reply_channel: Sender<Vec<B>>,
    },
}

impl<B: Blob + 'static> Debug for DaMsg<B> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaMsg::RemoveBlobs { .. } => {
                write!(f, "DaMsg::RemoveBlobs")
            }
            DaMsg::Get { .. } => {
                write!(f, "DaMsg::Get")
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

impl<Protocol, Backend, Network> DataAvailabilityService<Protocol, Backend, Network>
where
    Protocol: DaProtocol + Send + Sync,
    Backend: DaBackend<Blob = Protocol::Blob> + Send + Sync,
    Protocol::Settings: Clone + Send + Sync + 'static,
    Protocol::Blob: 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Protocol::Blob: Send,
    Protocol::Attestation: Send,
    <Backend::Blob as Blob>::Hash: Debug + Send + Sync,
    Network:
        NetworkAdapter<Blob = Protocol::Blob, Attestation = Protocol::Attestation> + Send + Sync,
{
    async fn handle_new_blob(
        da: &Protocol,
        backend: &Backend,
        adapter: &Network,
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

    async fn handle_da_msg(backend: &Backend, msg: DaMsg<Backend::Blob>) -> Result<(), DaError> {
        match msg {
            DaMsg::RemoveBlobs { blobs } => {
                futures::stream::iter(blobs)
                    .for_each_concurrent(None, |blob| async move {
                        if let Err(e) = backend.remove_blob(&blob).await {
                            tracing::debug!("Could not remove blob {blob:?} due to: {e:?}");
                        }
                    })
                    .await;
            }
            DaMsg::Get { ids, reply_channel } => {
                let res = ids.filter_map(|id| backend.get_blob(&id)).collect();
                if reply_channel.send(res).is_err() {
                    tracing::error!("Could not returns blobs");
                }
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
            backend,
            da,
            network_relay,
        } = self;

        let network_relay = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let adapter = Network::new(network_relay).await;
        let mut network_blobs = adapter.blob_stream().await;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(blob) = network_blobs.next() => {
                    if let Err(e) = Self::handle_new_blob(&da, &backend, &adapter, blob).await {
                        tracing::debug!("Failed to add a new received blob: {e:?}");
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    if let Err(e) = Self::handle_da_msg(&backend, msg).await {
                        tracing::debug!("Failed to handle da msg: {e:?}");
                    }
                }
                Some(msg) = lifecycle_stream.next() => {
                    if Self::should_stop_service(msg).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<P, B> {
    pub da_protocol: P,
    pub backend: B,
}

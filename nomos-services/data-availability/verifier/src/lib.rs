mod backend;
mod network;
mod storage;

// std
use nomos_storage::StorageService;
use std::fmt::Debug;
use storage::DaStorageAdapter;
// crates
use tokio_stream::StreamExt;
use tracing::error;
// internal
use backend::VerifierBackend;
use network::NetworkAdapter;
use nomos_network::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{NoMessage, Relay};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;

pub struct DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend,
    Backend::Settings: Clone,
    N: NetworkAdapter,
    N::Settings: Clone,
    S: DaStorageAdapter,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    storage_relay: Relay<StorageService<S::Backend>>,
    verifier: Backend,
}

impl<Backend, N, S> ServiceData for DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend,
    Backend::Settings: Clone,
    N: NetworkAdapter,
    N::Settings: Clone,
    S: DaStorageAdapter,
    S::Settings: Clone,
{
    const SERVICE_ID: ServiceId = "DaVerifier";
    type Settings = DaVerifierServiceSettings<Backend::Settings, N::Settings, S::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<Backend, N, S> ServiceCore for DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend + Send + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Backend::DaBlob: Debug + Send,
    Backend::Attestation: Debug + Send,
    N: NetworkAdapter<Blob = Backend::DaBlob, Attestation = Backend::Attestation> + Send + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    S: DaStorageAdapter<Blob = Backend::DaBlob, Attestation = Backend::Attestation>
        + Send
        + 'static,
    S::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let DaVerifierServiceSettings {
            verifier_settings, ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            network_relay,
            storage_relay,
            service_state,
            verifier: Backend::new(verifier_settings),
        })
    }
    async fn run(self) -> Result<(), DynError> {
        // This service will likely have to be modified later on.
        // Most probably the verifier itself need to be constructed/update for every message with
        // an updated list of the available nodes list, as it needs his own index coming from the
        // position of his bls public key landing in the above-mentioned list.
        let Self {
            network_relay,
            storage_relay,
            service_state,
            verifier,
        } = self;
        let DaVerifierServiceSettings {
            network_adapter_settings,
            storage_adapter_settings,
            ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = network_relay.connect().await?;
        let network_adapter = N::new(network_adapter_settings, network_relay).await;
        let mut blob_stream = network_adapter.blob_stream().await;
        let storage_relay = storage_relay.connect().await?;
        let storage_adapter = S::new(storage_adapter_settings, storage_relay).await;

        while let Some((blob, reply_channel)) = blob_stream.next().await {
            match storage_adapter.get_attestation(&blob).await {
                Ok(Some(attestation)) => {
                    if let Err(attestation) = reply_channel.send(attestation) {
                        error!("Error replying attestation {:?}", attestation);
                    }
                    continue;
                }
                Ok(None) => (), // Blob needs to be verified, pass through.
                Err(err) => {
                    error!("Error getting attestation for blob {blob:?} due to {err:?}");
                    continue;
                }
            }

            match verifier.verify(&blob) {
                Ok(attestation) => {
                    if let Err(err) = storage_adapter.add_blob(&blob, &attestation).await {
                        error!("Error storing blob {blob:?} due to {err:?}");
                    }
                    if let Err(attestation) = reply_channel.send(attestation) {
                        error!("Error replying attestation {attestation:?}");
                    }
                }
                Err(e) => {
                    error!("Received unverified blob {blob:?} due to {e:?}");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct DaVerifierServiceSettings<BackendSettings, NetworkSettings, StorageSettings> {
    verifier_settings: BackendSettings,
    network_adapter_settings: NetworkSettings,
    storage_adapter_settings: StorageSettings,
}

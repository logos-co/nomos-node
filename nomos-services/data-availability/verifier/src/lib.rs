mod backend;
mod network;

// std
use std::fmt::Debug;
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

pub struct DaVerifierService<Backend, N>
where
    Backend: VerifierBackend,
    Backend::Settings: Clone,
    N: NetworkAdapter,
    N::Settings: Clone,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    verifier: Backend,
}

#[derive(Clone)]
pub struct DaVerifierServiceSettings<BackendSettings, AdapterSettings> {
    verifier_settings: BackendSettings,
    network_adapter_settings: AdapterSettings,
}

impl<Backend, N> ServiceData for DaVerifierService<Backend, N>
where
    Backend: VerifierBackend,
    Backend::Settings: Clone,
    N: NetworkAdapter,
    N::Settings: Clone,
{
    const SERVICE_ID: ServiceId = "DaVerifier";
    type Settings = DaVerifierServiceSettings<Backend::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<Backend, N> ServiceCore for DaVerifierService<Backend, N>
where
    Backend: VerifierBackend + Send + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Backend::DaBlob: Debug,
    Backend::Attestation: Debug,
    N: NetworkAdapter<Blob = Backend::DaBlob, Attestation = Backend::Attestation> + Send + 'static,
    N::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let DaVerifierServiceSettings {
            verifier_settings, ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            network_relay,
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
            service_state,
            verifier,
        } = self;
        let DaVerifierServiceSettings {
            network_adapter_settings,
            ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = network_relay.connect().await?;
        let adapter = N::new(network_adapter_settings, network_relay).await;
        let mut blob_stream = adapter.blob_stream().await;
        while let Some((blob, reply_channel)) = blob_stream.next().await {
            // TODO: Verify if blob was already processed
            match verifier.verify(&blob) {
                Ok(attestation) => {
                    if let Err(attestation) = reply_channel.send(attestation) {
                        error!("Error replying attestation {:?}", attestation);
                    }
                    // TODO: Send blob to storage
                }
                Err(e) => {
                    error!("Received unverified blob {blob:?} due to {e:?}");
                }
            }
        }
        Ok(())
    }
}

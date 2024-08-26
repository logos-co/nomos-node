pub mod backend;
pub mod network;
pub mod storage;

use nomos_core::da::blob::Blob;
// std
use nomos_storage::StorageService;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use std::error::Error;
use std::fmt::{Debug, Formatter};
use storage::DaStorageAdapter;
use tokio::sync::oneshot::Sender;
// crates
use tokio_stream::StreamExt;
use tracing::{error, span, Instrument, Level};
// internal
use backend::SamplingBackend;
use network::NetworkAdapter;
use nomos_network::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;

const DA_SAMPLING_TAG: ServiceId = "DA-Sampling";
pub enum DaSamplingMsg<B, A> {
    SamplingRequest {
        reply_channel: Sender<Option<A>>,
    },
    SamplingResponse {
        blob: B,
        reply_channel: Sender<Option<A>>,
    },
}

impl<B: 'static, A: 'static> Debug for DaSamplingMsg<B, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaSamplingMsg::SamplingRequest { .. } => {
                write!(f, "DaSamplingMsg::SamplingRequest")
            }
            DaSamplingMsg::SamplingResponse { .. } => {
                write!(f, "DaSamplingMsg::SamplingResponse")
            }
        }
    }
}

impl<B: 'static, A: 'static> RelayMessage for DaSamplingMsg<B, A> {}

pub struct DaSamplingService<Backend, N, S>
where
    Backend: SamplingBackend,
    Backend::Settings: Clone,
    Backend::DaBlob: 'static,
    Backend::Error: Error,
    N: NetworkAdapter,
    N::Settings: Clone,
    S: DaStorageAdapter,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    storage_relay: Relay<StorageService<S::Backend>>,
    sampler: Backend,
}

impl<Backend, N, S> DaSamplingService<Backend, N, S>
where
    Backend: SamplingBackend + Send + 'static,
    Backend::DaBlob: Debug + Send,
    Backend::Error: Error + Send + Sync,
    Backend::Settings: Clone,
    N: NetworkAdapter<Blob = Backend::DaBlob, Attestation = ()> + Send + 'static,
    N::Settings: Clone,
    S: DaStorageAdapter<Blob = Backend::DaBlob, Attestation = ()> + Send + 'static,
{

    async fn trigger_sampling(
        self,
        sampler: &Backend,
        storage_adapter: &S,
        blob: &Backend::DaBlob,
    ) -> Result<(), DynError> {
        Ok(())
    }

    async fn handle_new_sample_response(
        sampler: &Backend,
        blob: &Backend::DaBlob,
    ) -> Result<(), DynError> {
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

impl<Backend, N, S> ServiceData for DaSamplingService<Backend, N, S>
where
    Backend: SamplingBackend,
    Backend::Settings: Clone,
    Backend::Error: Error,
    N: NetworkAdapter,
    N::Settings: Clone,
    S: DaStorageAdapter,
    S::Settings: Clone,
{
    const SERVICE_ID: ServiceId = DA_SAMPLING_TAG;
    type Settings = DaSamplingServiceSettings<Backend::Settings, N::Settings, S::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaSamplingMsg<Backend::DaBlob, ()>;
}

#[async_trait::async_trait]
impl<Backend, N, S> ServiceCore for DaSamplingService<Backend, N, S>
where
    Backend: SamplingBackend + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Backend::DaBlob: Debug + Send + Sync + 'static,
    Backend::Error: Error + Send + Sync + 'static,
    N: NetworkAdapter<Blob = Backend::DaBlob, Attestation = ()> + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    S: DaStorageAdapter<Blob = Backend::DaBlob, Attestation = ()> + Send + Sync + 'static,
    S::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let DaSamplingServiceSettings {
            sampling_settings, ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            network_relay,
            storage_relay,
            service_state,
            sampler: Backend::new(sampling_settings),
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
            mut service_state,
            sampler,
        } = self;

        let DaSamplingServiceSettings {
            network_adapter_settings,
            storage_adapter_settings,
            ..
        } = service_state.settings_reader.get_updated_settings();

        let network_relay = network_relay.connect().await?;
        let network_adapter = N::new(network_adapter_settings, network_relay).await;
        let mut blob_stream = network_adapter.blob_stream().await;

        let storage_relay = storage_relay.connect().await?;
        let storage_adapter = S::new(storage_adapter_settings, storage_relay).await;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        async {
            loop {
                tokio::select! {
                    Some(blob) = blob_stream.next() => {
                        match Self::handle_new_sample_request(&sampler,&storage_adapter, &blob).await {
                            Ok(attestation) => if let Err(err) = network_adapter.send_attestation(attestation).await {
                                error!("Error replying attestation {err:?}");
                            },
                            Err(err) => error!("Error handling blob {blob:?} due to {err:?}"),
                        }
                    }
                    Some(msg) = service_state.inbound_relay.recv() => {
                            let DaSamplingMsg::AddBlob { blob, reply_channel } = msg;
                            match Self::handle_new_sample_request(&sampler, &storage_adapter, &blob).await {
                                Ok(attestation) => if let Err(err) = reply_channel.send(Some(attestation)) {
                                    error!("Error replying attestation {err:?}");
                                },
                                Err(err) => {
                                    error!("Error handling blob {blob:?} due to {err:?}");
                                    if let Err(err) = reply_channel.send(None) {
                                        error!("Error replying attestation {err:?}");
                                    }
                                },
                            };
                        }
                    Some(msg) = lifecycle_stream.next() => {
                        if Self::should_stop_service(msg).await {
                            break;
                        }
                    }
                }
            }
        }.instrument(span!(Level::TRACE, DA_VERIFIER_TAG)).await;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DaSamplingServiceSettings<BackendSettings, NetworkSettings, StorageSettings> {
    pub sampling_settings: BackendSettings,
    pub network_adapter_settings: NetworkSettings,
    pub storage_adapter_settings: StorageSettings,
}

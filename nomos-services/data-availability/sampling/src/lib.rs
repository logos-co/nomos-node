pub mod backend;
pub mod network;

// std
use overwatch_rs::services::life_cycle::LifecycleMessage;
use std::fmt::{Debug, Formatter};
// crates
use tokio_stream::StreamExt;
use tracing::{error, span, Instrument, Level};
// internal
use backend::DaSamplingServiceBackend;
use network::NetworkAdapter;
use nomos_da_network_service::backends::libp2p::validator::SamplingEvent;
use nomos_da_network_service::backends::NetworkBackend;
use nomos_da_network_service::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;

const DA_SAMPLING_TAG: ServiceId = "DA-Sampling";
pub enum DaSamplingServiceMsg<B, BI> {
    BlobSamplingCompletedSuccess { blob: B, blob_id: BI },
    BlobSamplingCompletedFailure { blob: B, blob_id: BI },
}

impl<B: 'static, A: 'static> Debug for DaSamplingServiceMsg<B, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaSamplingServiceMsg::BlobSamplingCompletedSuccess { .. } => {
                write!(f, "DaSamplingServiceMsg::BlobSamplingCompletedSuccess")
            }
            DaSamplingServiceMsg::BlobSamplingCompletedFailure { .. } => {
                write!(f, "DaSamplingServiceMsg::BlobSamplingCompletedFailure")
            }
        }
    }
}

impl<B: 'static, A: 'static> RelayMessage for DaSamplingServiceMsg<B, A> {}

pub struct DaSamplingService<Backend, N, S>
where
    Backend: DaSamplingServiceBackend<Backend, S> + NetworkBackend,
    N: NetworkAdapter,
    N::Settings: Clone,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    sampler: Backend,
}

impl<Backend, N, S> DaSamplingService<Backend, N, S>
where
    Backend: DaSamplingServiceBackend + Send + 'static,
    Backend::Settings: Clone,
    N: NetworkAdapter + Send + 'static,
    N::Settings: Clone,
{
    async fn handle_new_sample_response(
        sampler: &Backend,
        network_adapter: &N,
        evt: &SamplingEvent,
    ) -> Result<(), DynError> {
        match evt {
            SamplingEvent::SamplingSuccess => {
                let &SamplingEvent::SamplingSuccess { blob_id, blob } = evt;
                network_adapter.handle_sampling_event(true, blob_id, 99)?
            }

            SamplingEvent::SamplingError => {
                let &SamplingEvent::SamplingError { error } = evt;
                // TODO!!! how to get the blob_id and the subnet_id from the sample error?
                // we need that (I think) to update the network adapter's state
                network_adapter.handle_sampling_event(false, 42, 42)?
            }
        }
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
    Backend: DaSamplingServiceBackend<Backend, S>,
    Backend::Settings: Clone,
    N: NetworkAdapter,
    N::Settings: Clone,
{
    const SERVICE_ID: ServiceId = DA_SAMPLING_TAG;
    type Settings = DaSamplingServiceSettings<Backend::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    //type Message = DaSamplingServiceMsg<Backend::Blob, Backend::BlobId>;
    type Message = SamplingEvent;
}

#[async_trait::async_trait]
impl<Backend, N, S> ServiceCore for DaSamplingService<Backend, N, S>
where
    Backend: DaSamplingServiceBackend + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    N: NetworkAdapter + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let DaSamplingServiceSettings {
            sampling_settings,
            network_adapter_settings,
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = service_state.overwatch_handle.relay();
        let network_adapter = NetworkAdapter::new(network_adapter_settings, network_relay);
        Ok(Self {
            network_relay,
            service_state,
            sampler: Backend::new(sampling_settings, network_adapter),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        // This service will likely have to be modified later on.
        // Most probably the verifier itself need to be constructed/update for every message with
        // an updated list of the available nodes list, as it needs his own index coming from the
        // position of his bls public key landing in the above-mentioned list.
        let Self {
            network_relay,
            mut service_state,
            sampler,
        } = self;

        let DaSamplingServiceSettings {
            sampling_settings,
            network_adapter_settings,
        } = service_state.settings_reader.get_updated_settings();

        let network_relay = network_relay.connect().await?;
        let network_adapter = N::new(network_adapter_settings, network_relay).await;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        async {
            loop {
                tokio::select! {
                    Some(msg) = service_state.inbound_relay.recv() => {
                            match Self::handle_new_sample_response(&sampler, &network_adapter, &msg).await {
                                Ok(()) => {}, // what should happen here?
                                Err(err) => {
                                    error!("Error handling new sampling event, due to {err:?}");
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
        }.instrument(span!(Level::TRACE, DA_SAMPLING_TAG)).await;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct DaSamplingServiceSettings<BackendSettings, NetworkSettings> {
    pub sampling_settings: BackendSettings,
    pub network_adapter_settings: NetworkSettings,
}

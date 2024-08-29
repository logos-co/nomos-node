pub mod backend;
pub mod network;

// std
use std::collections::BTreeSet;
use std::fmt::Debug;

// crates
use rand::prelude::*;
use tokio_stream::StreamExt;
use tracing::{error, span, Instrument, Level};
// internal
use backend::{DaSamplingServiceBackend, SamplingState};
use kzgrs_backend::common::blob::DaBlob;
use network::NetworkAdapter;
use nomos_core::da::BlobId;
use nomos_da_network_service::backends::libp2p::validator::SamplingEvent;
use nomos_da_network_service::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use tokio::sync::oneshot;

const DA_SAMPLING_TAG: ServiceId = "DA-Sampling";

#[derive(Debug)]
pub enum DaSamplingServiceMsg<BlobId> {
    TriggerSampling {
        blob_id: BlobId,
    },
    GetValidatedBlobs {
        reply_channel: oneshot::Sender<BTreeSet<BlobId>>,
    },
    MarkInBlock {
        blobs_id: Vec<BlobId>,
    },
}

#[derive(Debug, Clone)]
pub struct DaSamplingServiceSettings<BackendSettings, NetworkSettings> {
    pub sampling_settings: BackendSettings,
    pub network_adapter_settings: NetworkSettings,
}

impl<B: 'static> RelayMessage for DaSamplingServiceMsg<B> {}

pub struct DaSamplingService<Backend, N, R>
where
    R: SeedableRng + RngCore,
    Backend: DaSamplingServiceBackend<R> + Send,
    Backend::Settings: Clone,
    Backend::Blob: Debug + 'static,
    Backend::BlobId: Debug + 'static,
    N: NetworkAdapter,
    N::Settings: Clone,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    sampler: Backend,
}

impl<Backend, N, R> DaSamplingService<Backend, N, R>
where
    R: SeedableRng + RngCore,
    Backend: DaSamplingServiceBackend<R, BlobId = BlobId, Blob = DaBlob> + Send + 'static,
    Backend::Settings: Clone,
    N: NetworkAdapter + Send + 'static,
    N::Settings: Clone,
{
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

    async fn handle_service_message(
        msg: <Self as ServiceData>::Message,
        network_adapter: &mut N,
        sampler: &mut Backend,
    ) {
        match msg {
            DaSamplingServiceMsg::TriggerSampling { blob_id } => {
                if let SamplingState::Init(sampling_subnets) = sampler.init_sampling(blob_id).await
                {
                    if let Err(e) = network_adapter
                        .start_sampling(blob_id, &sampling_subnets)
                        .await
                    {
                        // we can short circuit the failure from beginning
                        sampler.handle_sampling_error(blob_id).await;
                        error!("Error sampling for BlobId: {blob_id:?}: {e}");
                    }
                }
            }
            DaSamplingServiceMsg::GetValidatedBlobs { reply_channel } => {
                let validated_blobs = sampler.get_validated_blobs().await;
                if let Err(_e) = reply_channel.send(validated_blobs) {
                    error!("Error repliying validated blobs request");
                }
            }
            DaSamplingServiceMsg::MarkInBlock { blobs_id } => {
                sampler.mark_completed(&blobs_id).await;
            }
        }
    }

    async fn handle_sampling_message(event: SamplingEvent, sampler: &mut Backend) {
        match event {
            SamplingEvent::SamplingSuccess { blob_id, blob } => {
                sampler.handle_sampling_success(blob_id, *blob).await;
            }
            SamplingEvent::SamplingError { error } => {
                if let Some(blob_id) = error.blob_id() {
                    sampler.handle_sampling_error(*blob_id).await;
                    return;
                }
                error!("Error while sampling: {error}");
            }
        }
    }
}

impl<Backend, N, R> ServiceData for DaSamplingService<Backend, N, R>
where
    R: SeedableRng + RngCore,
    Backend: DaSamplingServiceBackend<R> + Send,
    Backend::Settings: Clone,
    Backend::Blob: Debug + 'static,
    Backend::BlobId: Debug + 'static,
    N: NetworkAdapter,
    N::Settings: Clone,
{
    const SERVICE_ID: ServiceId = DA_SAMPLING_TAG;
    type Settings = DaSamplingServiceSettings<Backend::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaSamplingServiceMsg<Backend::BlobId>;
}

#[async_trait::async_trait]
impl<Backend, N, R> ServiceCore for DaSamplingService<Backend, N, R>
where
    R: SeedableRng + RngCore,
    Backend: DaSamplingServiceBackend<R, BlobId = BlobId, Blob = DaBlob> + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    N: NetworkAdapter + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let DaSamplingServiceSettings {
            sampling_settings, ..
        } = service_state.settings_reader.get_updated_settings();

        let network_relay = service_state.overwatch_handle.relay();
        let rng = R::from_entropy();

        Ok(Self {
            network_relay,
            service_state,
            sampler: Backend::new(sampling_settings, rng),
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
            mut sampler,
        } = self;
        let DaSamplingServiceSettings { .. } = service_state.settings_reader.get_updated_settings();

        let network_relay = network_relay.connect().await?;
        let mut network_adapter = N::new(network_relay).await;

        let mut sampling_message_stream = network_adapter.listen_to_sampling_messages().await?;
        let mut next_prune_tick = sampler.prune_interval();

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        async {
            loop {
                tokio::select! {
                    Some(service_message) = service_state.inbound_relay.recv() => {
                        Self::handle_service_message(service_message, &mut network_adapter, &mut sampler).await;
                    }
                    Some(sampling_message) = sampling_message_stream.next() => {
                        Self::handle_sampling_message(sampling_message, &mut sampler).await;
                    }
                    Some(msg) = lifecycle_stream.next() => {
                        if Self::should_stop_service(msg).await {
                            break;
                        }
                    }
                    // cleanup not on time samples
                    _ = next_prune_tick.tick() => {
                        sampler.prune();
                    }

                }
            }
        }
        .instrument(span!(Level::TRACE, DA_SAMPLING_TAG))
        .await;

        Ok(())
    }
}

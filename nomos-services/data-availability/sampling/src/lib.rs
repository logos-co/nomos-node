pub mod api;
pub mod backend;
pub mod network;
pub mod storage;

use std::{collections::BTreeSet, fmt::Debug, marker::PhantomData, sync::Arc};

use backend::{DaSamplingServiceBackend, SamplingState};
use kzgrs_backend::common::blob::{DaBlob, DaBlobSharedCommitments, DaLightBlob};
use network::NetworkAdapter;
use nomos_core::da::{blob::Blob, BlobId, DaVerifier};
use nomos_da_network_core::protocols::sampling::behaviour::SamplingError;
use nomos_da_network_service::{backends::libp2p::common::SamplingEvent, NetworkService};
use nomos_da_verifier::{
    backend::VerifierBackend as VerifierBackendTrait, DaVerifierMsg, DaVerifierService,
};
use nomos_storage::StorageService;
use nomos_tracing::{error_with_id, info_with_id};
use overwatch::{
    services::{
        relay::{OutboundRelay, RelayMessage},
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    DynError, OpaqueServiceStateHandle,
};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use services_utils::overwatch::lifecycle;
use storage::DaStorageAdapter;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tracing::{error, instrument};

use crate::api::ApiAdapter as ApiAdapterTrait;

const DA_SAMPLING_TAG: ServiceId = "DA-Sampling";

type VerifierRelay<DaVerifierBackend> = OutboundRelay<
    DaVerifierMsg<
        <<DaVerifierBackend as DaVerifier>::DaBlob as Blob>::SharedCommitments,
        <<DaVerifierBackend as DaVerifier>::DaBlob as Blob>::LightBlob,
        <DaVerifierBackend as DaVerifier>::DaBlob,
        (),
    >,
>;

type VerifierMessage = DaVerifierMsg<DaBlobSharedCommitments, DaLightBlob, DaBlob, ()>;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaSamplingServiceSettings<BackendSettings, ApiAdapterSettings> {
    pub sampling_settings: BackendSettings,
    pub api_adapter_settings: ApiAdapterSettings,
}

impl<B: 'static> RelayMessage for DaSamplingServiceMsg<B> {}

pub struct DaSamplingService<
    SamplingBackend,
    SamplingRng,
    SamplingNetwork,
    SamplingStorage,
    ApiAdapter,
    VerifierBackend,
    VerifierNetwork,
    VerifierStorage,
> where
    SamplingBackend: DaSamplingServiceBackend<SamplingRng>,
    SamplingRng: Rng,
    SamplingNetwork: NetworkAdapter,
    SamplingStorage: DaStorageAdapter,
    ApiAdapter: ApiAdapterTrait,
{
    service_state: OpaqueServiceStateHandle<Self>,
    #[expect(clippy::type_complexity, reason = "No other way around this for now.")]
    _phantom: PhantomData<(
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    )>,
}

impl<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    >
    DaSamplingService<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    >
where
    SamplingBackend: DaSamplingServiceBackend<SamplingRng>,
    SamplingRng: Rng,
    SamplingNetwork: NetworkAdapter,
    SamplingStorage: DaStorageAdapter,
    ApiAdapter: ApiAdapterTrait,
{
    #[must_use]
    pub const fn new(service_state: OpaqueServiceStateHandle<Self>) -> Self {
        Self {
            service_state,
            _phantom: PhantomData,
        }
    }
}

impl<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    >
    DaSamplingService<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    >
where
    SamplingBackend: DaSamplingServiceBackend<
        SamplingRng,
        BlobId = BlobId,
        Blob = DaBlob,
        SharedCommitments = DaBlobSharedCommitments,
    >,
    SamplingBackend::Settings: Clone,
    SamplingRng: Rng + SeedableRng,
    SamplingNetwork: NetworkAdapter,
    SamplingStorage: DaStorageAdapter<Blob = DaBlob>,
    ApiAdapter: ApiAdapterTrait<
        Blob = SamplingBackend::Blob,
        BlobId = SamplingBackend::BlobId,
        Commitments = SamplingBackend::SharedCommitments,
    >,
    ApiAdapter::Settings: Clone,
    VerifierBackend: VerifierBackendTrait<DaBlob = DaBlob>,
{
    #[instrument(skip_all)]
    async fn handle_service_message(
        msg: <Self as ServiceData>::Message,
        network_adapter: &mut SamplingNetwork,
        storage_adapter: &SamplingStorage,
        api_adapter: &ApiAdapter,
        sampler: &mut SamplingBackend,
    ) {
        match msg {
            DaSamplingServiceMsg::TriggerSampling { blob_id } => {
                if let SamplingState::Init(sampling_subnets) = sampler.init_sampling(blob_id).await
                {
                    info_with_id!(blob_id, "InitSampling");
                    if let Some(commitments) =
                        Self::request_commitments(storage_adapter, api_adapter, blob_id).await
                    {
                        sampler.add_commitments(&blob_id, commitments);
                    } else {
                        error_with_id!(blob_id, "Error getting commitments");
                        sampler.handle_sampling_error(blob_id).await;
                        return;
                    }

                    if let Err(e) = network_adapter
                        .start_sampling(blob_id, &sampling_subnets)
                        .await
                    {
                        // we can short circuit the failure from beginning
                        sampler.handle_sampling_error(blob_id).await;
                        error_with_id!(blob_id, "Error sampling for BlobId: {blob_id:?}: {e}");
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

    #[instrument(skip_all)]
    async fn handle_sampling_message(
        event: SamplingEvent,
        sampler: &mut SamplingBackend,
        storage_adapter: &SamplingStorage,
        verifier_relay: &VerifierRelay<VerifierBackend>,
    ) {
        match event {
            SamplingEvent::SamplingSuccess {
                blob_id,
                light_blob,
            } => {
                info_with_id!(blob_id, "SamplingSuccess");
                let Some(commitments) = sampler.get_commitments(&blob_id) else {
                    error_with_id!(blob_id, "Error getting commitments for blob");
                    sampler.handle_sampling_error(blob_id).await;
                    return;
                };
                if Self::verify_blob(verifier_relay, commitments, light_blob.clone())
                    .await
                    .is_ok()
                {
                    sampler
                        .handle_sampling_success(blob_id, light_blob.column_idx)
                        .await;
                } else {
                    error_with_id!(blob_id, "SamplingError");
                    sampler.handle_sampling_error(blob_id).await;
                }
                return;
            }
            SamplingEvent::SamplingError { error } => {
                if let Some(blob_id) = error.blob_id() {
                    error_with_id!(blob_id, "SamplingError");
                    if let SamplingError::BlobNotFound { .. } = error {
                        sampler.handle_sampling_error(*blob_id).await;
                        return;
                    }
                }
                error!("Error while sampling: {error}");
            }
            SamplingEvent::SamplingRequest {
                blob_id,
                column_idx,
                response_sender,
            } => {
                info_with_id!(blob_id, "SamplingRequest");
                let maybe_blob = storage_adapter
                    .get_light_blob(blob_id, column_idx)
                    .await
                    .map_err(|error| {
                        error!("Failed to get blob from storage adapter: {error}");
                        None::<SamplingBackend::Blob>
                    })
                    .ok()
                    .flatten();

                if response_sender.send(maybe_blob).await.is_err() {
                    error!("Error sending sampling response");
                }
            }
        }
    }

    async fn request_commitments(
        storage_adapter: &SamplingStorage,
        api_request: &ApiAdapter,
        blob_id: SamplingBackend::BlobId,
    ) -> Option<DaBlobSharedCommitments> {
        // First try to get from storage which most of the time should be the case
        if let Ok(Some(commitments)) = storage_adapter.get_commitments(blob_id).await {
            return Some(commitments);
        }

        // Fall back to API request
        let (reply_sender, reply_channel) = oneshot::channel();
        if let Err(e) = api_request.request_commitments(blob_id, reply_sender).await {
            error!("Error sending request to API backend: {e}");
        }

        reply_channel.await.ok().flatten()
    }

    async fn verify_blob(
        verifier_relay: &OutboundRelay<VerifierMessage>,
        commitments: Arc<DaBlobSharedCommitments>,
        light_blob: Box<DaLightBlob>,
    ) -> Result<(), DynError> {
        let (reply_sender, reply_channel) = oneshot::channel();
        verifier_relay
            .send(DaVerifierMsg::VerifyBlob {
                commitments,
                light_blob,
                reply_channel: reply_sender,
            })
            .await
            .expect("Failed to send verify blob message to verifier relay");
        reply_channel
            .await
            .expect("Failed to receive reply blob message from verifierrelay")
    }
}

impl<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    > ServiceData
    for DaSamplingService<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    >
where
    SamplingBackend: DaSamplingServiceBackend<SamplingRng>,
    SamplingRng: Rng,
    SamplingNetwork: NetworkAdapter,
    SamplingStorage: DaStorageAdapter,
    ApiAdapter: ApiAdapterTrait,
{
    const SERVICE_ID: ServiceId = DA_SAMPLING_TAG;
    type Settings = DaSamplingServiceSettings<SamplingBackend::Settings, ApiAdapter::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State, Self::Settings>;
    type Message = DaSamplingServiceMsg<SamplingBackend::BlobId>;
}

#[async_trait::async_trait]
impl<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    > ServiceCore
    for DaSamplingService<
        SamplingBackend,
        SamplingRng,
        SamplingNetwork,
        SamplingStorage,
        ApiAdapter,
        VerifierBackend,
        VerifierNetwork,
        VerifierStorage,
    >
where
    SamplingBackend: DaSamplingServiceBackend<
            SamplingRng,
            BlobId = BlobId,
            Blob = DaBlob,
            SharedCommitments = DaBlobSharedCommitments,
        > + Send,
    SamplingBackend::Settings: Clone + Send + Sync,
    SamplingRng: Rng + SeedableRng + Send,
    SamplingNetwork: NetworkAdapter + Send,
    SamplingNetwork::Settings: Send + Sync,
    SamplingStorage: DaStorageAdapter<Blob = DaBlob> + Send + Sync,
    ApiAdapter: ApiAdapterTrait<
            Blob = SamplingBackend::Blob,
            BlobId = SamplingBackend::BlobId,
            Commitments = SamplingBackend::SharedCommitments,
        > + Send
        + Sync,
    ApiAdapter::Settings: Clone + Send + Sync,
    VerifierBackend:
        nomos_da_verifier::backend::VerifierBackend<DaBlob = SamplingBackend::Blob> + Send,
    VerifierBackend::Settings: Clone,
    VerifierNetwork: nomos_da_verifier::network::NetworkAdapter + Send,
    VerifierNetwork::Settings: Clone,
    VerifierStorage: nomos_da_verifier::storage::DaStorageAdapter + Send,
{
    fn init(
        service_state: OpaqueServiceStateHandle<Self>,
        _init_state: Self::State,
    ) -> Result<Self, DynError> {
        Ok(Self::new(service_state))
    }

    async fn run(mut self) -> Result<(), DynError> {
        // This service will likely have to be modified later on.
        // Most probably the verifier itself need to be constructed/update for every
        // message with an updated list of the available nodes list, as it needs
        // his own index coming from the position of his bls public key landing
        // in the above-mentioned list.
        let DaSamplingServiceSettings {
            sampling_settings,
            api_adapter_settings,
            ..
        } = self.service_state.settings_reader.get_updated_settings();

        let network_relay = self
            .service_state
            .overwatch_handle
            .relay::<NetworkService<_>>()
            .connect()
            .await?;
        let mut network_adapter = SamplingNetwork::new(network_relay).await;

        let storage_relay = self
            .service_state
            .overwatch_handle
            .relay::<StorageService<_>>()
            .connect()
            .await?;
        let storage_adapter = SamplingStorage::new(storage_relay).await;

        let verifier_relay = self
            .service_state
            .overwatch_handle
            .relay::<DaVerifierService<VerifierBackend, VerifierNetwork, VerifierStorage>>()
            .connect()
            .await?;

        let api_adapter = ApiAdapter::new(api_adapter_settings);

        let rng = SamplingRng::from_entropy();
        let mut sampler = SamplingBackend::new(sampling_settings, rng);
        let mut next_prune_tick = sampler.prune_interval();

        let mut sampling_message_stream = network_adapter.listen_to_sampling_messages().await?;

        let mut lifecycle_stream = self.service_state.lifecycle_handle.message_stream();
        #[expect(
            clippy::redundant_pub_crate,
            reason = "Generated by `tokio::select` macro."
        )]
        loop {
            tokio::select! {
                Some(service_message) = self.service_state.inbound_relay.recv() => {
                    Self::handle_service_message(service_message, &mut network_adapter,  &storage_adapter, &api_adapter, &mut sampler).await;
                }
                Some(sampling_message) = sampling_message_stream.next() => {
                    Self::handle_sampling_message(sampling_message, &mut sampler, &storage_adapter, &verifier_relay).await;
                }
                Some(msg) = lifecycle_stream.next() => {
                    if lifecycle::should_stop_service::<Self>(&msg) {
                        break;
                    }
                }
                // cleanup not on time samples
                _ = next_prune_tick.tick() => {
                    sampler.prune();
                }

            }
        }

        Ok(())
    }
}

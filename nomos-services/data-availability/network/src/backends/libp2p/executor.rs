use std::{fmt::Debug, marker::PhantomData, pin::Pin};

use futures::{
    future::Aborted,
    stream::{AbortHandle, Abortable},
    Stream, StreamExt,
};
use kzgrs_backend::common::blob::DaBlob;
use libp2p::PeerId;
use log::error;
use nomos_core::da::BlobId;
use nomos_da_network_core::{
    protocols::dispersal::executor::behaviour::DispersalExecutorEvent,
    swarm::executor::ExecutorSwarm, SubnetworkId,
};
use nomos_libp2p::ed25519;
use nomos_tracing::info_with_id;
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;
use tokio::{
    sync::{broadcast, mpsc::UnboundedSender},
    task::JoinHandle,
};
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};
use tracing::instrument;

use crate::backends::{
    libp2p::common::{
        handle_sample_request, handle_validator_events_stream, DaNetworkBackendSettings,
        SamplingEvent, BROADCAST_CHANNEL_SIZE,
    },
    NetworkBackend,
};

/// Message that the backend replies to
#[derive(Debug)]
pub enum ExecutorDaNetworkMessage {
    /// Kickstart a network sapling
    RequestSample {
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
    },
    RequestDispersal {
        subnetwork_id: SubnetworkId,
        da_blob: Box<DaBlob>,
    },
}

/// Events types to subscribe to
/// * Sampling: Incoming sampling events [success/fail]
/// * Incoming blobs to be verified
#[derive(Debug)]
pub enum DaNetworkEventKind {
    Sampling,
    Verifying,
    Dispersal,
}

/// DA network incoming events
#[derive(Debug)]
pub enum DaNetworkEvent {
    Sampling(SamplingEvent),
    Verifying(Box<DaBlob>),
    Dispersal(DispersalExecutorEvent),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaNetworkExecutorBackendSettings<Membership> {
    pub validator_settings: DaNetworkBackendSettings<Membership>,
    pub num_subnets: u16,
}

/// DA network backend for validators
/// Internally uses a libp2p swarm composed of the [`ExecutorBehaviour`]
/// It forwards network messages to the corresponding subscription
/// channels/streams
pub struct DaNetworkExecutorBackend<Membership>
where
    Membership: MembershipHandler,
{
    task: (AbortHandle, JoinHandle<Result<(), Aborted>>),
    verifier_replies_task: (AbortHandle, JoinHandle<Result<(), Aborted>>),
    executor_replies_task: (AbortHandle, JoinHandle<Result<(), Aborted>>),
    sampling_request_channel: UnboundedSender<(SubnetworkId, BlobId)>,
    sampling_broadcast_receiver: broadcast::Receiver<SamplingEvent>,
    verifying_broadcast_receiver: broadcast::Receiver<DaBlob>,
    dispersal_broadcast_receiver: broadcast::Receiver<DispersalExecutorEvent>,
    dispersal_blobs_sender: UnboundedSender<(Membership::NetworkId, DaBlob)>,
    _membership: PhantomData<Membership>,
}

#[async_trait::async_trait]
impl<Membership> NetworkBackend for DaNetworkExecutorBackend<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type Settings = DaNetworkExecutorBackendSettings<Membership>;
    type State = NoState<Self::Settings>;
    type Message = ExecutorDaNetworkMessage;
    type EventKind = DaNetworkEventKind;
    type NetworkEvent = DaNetworkEvent;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let keypair = libp2p::identity::Keypair::from(ed25519::Keypair::from(
            config.validator_settings.node_key.clone(),
        ));
        let (mut executor_swarm, executor_events_stream) = ExecutorSwarm::new(
            keypair,
            config.validator_settings.membership.clone(),
            config
                .validator_settings
                .addresses
                .clone()
                .into_iter()
                .collect(),
            config.validator_settings.policy_settings.clone(),
            config.validator_settings.monitor_settings.clone(),
            config.validator_settings.balancer_interval,
            config.validator_settings.redial_cooldown,
        );
        let address = config.validator_settings.listening_address.clone();
        // put swarm to listen at the specified configuration address
        executor_swarm
            .protocol_swarm_mut()
            .listen_on(address.clone())
            .unwrap_or_else(|e| {
                panic!("Error listening on DA network with address {address}: {e}")
            });

        let sampling_request_channel = executor_swarm.sample_request_channel();
        let dispersal_blobs_sender = executor_swarm.dispersal_blobs_channel();

        let (task_abort_handle, abort_registration) = AbortHandle::new_pair();
        let task = (
            task_abort_handle,
            overwatch_handle
                .runtime()
                .spawn(Abortable::new(executor_swarm.run(), abort_registration)),
        );

        let (sampling_broadcast_sender, sampling_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let (verifying_broadcast_sender, verifying_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let (dispersal_broadcast_sender, dispersal_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let (verifier_replies_task_abort_handle, verifier_replies_task_abort_registration) =
            AbortHandle::new_pair();
        let verifier_replies_task = (
            verifier_replies_task_abort_handle,
            overwatch_handle.runtime().spawn(Abortable::new(
                handle_validator_events_stream(
                    executor_events_stream.validator_events_stream,
                    sampling_broadcast_sender,
                    verifying_broadcast_sender,
                ),
                verifier_replies_task_abort_registration,
            )),
        );
        let (executor_replies_task_abort_handle, executor_replies_task_abort_registration) =
            AbortHandle::new_pair();

        let executor_replies_task = (
            executor_replies_task_abort_handle,
            overwatch_handle.runtime().spawn(Abortable::new(
                handle_executor_dispersal_events_stream(
                    executor_events_stream.dispersal_events_receiver,
                    dispersal_broadcast_sender,
                ),
                executor_replies_task_abort_registration,
            )),
        );

        Self {
            task,
            verifier_replies_task,
            executor_replies_task,
            sampling_request_channel,
            sampling_broadcast_receiver,
            verifying_broadcast_receiver,
            dispersal_broadcast_receiver,
            dispersal_blobs_sender,
            _membership: Default::default(),
        }
    }

    fn shutdown(&mut self) {
        let Self {
            task: (task_handle, _),
            verifier_replies_task: (verifier_handle, _),
            executor_replies_task: (executor_handle, _),
            ..
        } = self;
        task_handle.abort();
        verifier_handle.abort();
        executor_handle.abort();
    }

    #[instrument(skip_all)]
    async fn process(&self, msg: Self::Message) {
        match msg {
            ExecutorDaNetworkMessage::RequestSample {
                subnetwork_id,
                blob_id,
            } => {
                info_with_id!(&blob_id, "RequestSample");
                handle_sample_request(&self.sampling_request_channel, subnetwork_id, blob_id).await;
            }
            ExecutorDaNetworkMessage::RequestDispersal {
                subnetwork_id,
                da_blob,
            } => {
                info_with_id!(&da_blob.id(), "RequestDispersal");
                if let Err(e) = self.dispersal_blobs_sender.send((subnetwork_id, *da_blob)) {
                    error!("Could not send internal blob to underlying dispersal behaviour: {e}");
                }
            }
        }
    }

    async fn subscribe(
        &mut self,
        event: Self::EventKind,
    ) -> Pin<Box<dyn Stream<Item = Self::NetworkEvent> + Send>> {
        match event {
            DaNetworkEventKind::Sampling => Box::pin(
                BroadcastStream::new(self.sampling_broadcast_receiver.resubscribe())
                    .filter_map(|event| async { event.ok() })
                    .map(Self::NetworkEvent::Sampling),
            ),
            DaNetworkEventKind::Verifying => Box::pin(
                BroadcastStream::new(self.verifying_broadcast_receiver.resubscribe())
                    .filter_map(|event| async { event.ok() })
                    .map(|blob| Self::NetworkEvent::Verifying(Box::new(blob))),
            ),
            DaNetworkEventKind::Dispersal => Box::pin(
                BroadcastStream::new(self.dispersal_broadcast_receiver.resubscribe())
                    .filter_map(|event| async { event.ok() })
                    .map(Self::NetworkEvent::Dispersal),
            ),
        }
    }
}

async fn handle_executor_dispersal_events_stream(
    mut dispersal_events_receiver: UnboundedReceiverStream<DispersalExecutorEvent>,
    dispersal_broadcast_sender: broadcast::Sender<DispersalExecutorEvent>,
) {
    while let Some(event) = dispersal_events_receiver.next().await {
        if let Err(e) = dispersal_broadcast_sender.send(event) {
            error!("Error forwarding internal dispersal executor event: {e}");
        }
    }
}

use crate::backends::NetworkBackend;
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use libp2p::identity::Keypair;
use libp2p::PeerId;
use log::error;
use nomos_da_network_core::swarm::validator::{ValidatorEventsStream, ValidatorSwarm};
use nomos_da_network_core::SubnetworkId;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use overwatch_rs::services::state::NoState;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::broadcast;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::BroadcastStream;

type BlobId = [u8; 32];
type ColumnIdx = u32;

const BROADCAST_CHANNEL_SIZE: usize = 128;

#[derive(Debug)]
pub enum DaNetworkMessage {
    RequestSample {
        subnetwork_id: ColumnIdx,
        blob_id: BlobId,
    },
}

#[derive(Debug)]
pub enum DaNetworkEventKind {
    Sampling,
    Verifying,
}

#[derive(Debug, Clone)]
pub enum SamplingEvent {
    SamplingSuccess {},
    SamplingError {},
}

#[derive(Debug, Clone)]
pub enum VerifyingEvent {
    Dispersal {},
    Replication {},
}

#[derive(Debug)]
pub enum DaNetworkEvent {
    Sampling(SamplingEvent),
    Verifying(VerifyingEvent),
}

pub struct DaNetworkValidatorBackend<Membership> {
    task: JoinHandle<()>,
    replies_task: JoinHandle<()>,
    sampling_request_channel: UnboundedSender<(SubnetworkId, BlobId)>,
    sampling_broadcast_sender: broadcast::Sender<SamplingEvent>,
    sampling_broadcast_receiver: broadcast::Receiver<SamplingEvent>,
    verifying_broadcast_sender: broadcast::Sender<VerifyingEvent>,
    verifying_broadcast_receiver: broadcast::Receiver<VerifyingEvent>,
    _membership: PhantomData<Membership>,
}

#[derive(Clone, Debug)]
pub struct DaNetworkValidatorBackendSettings<Membership> {
    key: Keypair,
    membership: Membership,
}

impl<Membership> DaNetworkValidatorBackend<Membership> {
    async fn handle_sample_request(&self, subnetwork_id: SubnetworkId, blob_id: BlobId) {
        if let Err(SendError((subnetwork_id, blob_id))) =
            self.sampling_request_channel.send((subnetwork_id, blob_id))
        {
            error!(
                "Error requesting sample for subnetwork id : {subnetwork_id}, blob_id: {blob_id:?}"
            );
        }
    }
}

#[async_trait::async_trait]
impl<Membership> NetworkBackend for DaNetworkValidatorBackend<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type Settings = DaNetworkValidatorBackendSettings<Membership>;
    type State = NoState<Self::Settings>;
    type Message = DaNetworkMessage;
    type EventKind = DaNetworkEventKind;
    type NetworkEvent = DaNetworkEvent;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let (mut validator_swarm, events_streams) =
            ValidatorSwarm::new(config.key, config.membership);
        let sampling_request_channel = validator_swarm
            .protocol_swarm()
            .behaviour()
            .sampling_behaviour()
            .sample_request_channel();

        let task = overwatch_handle.runtime().spawn(validator_swarm.run());
        let replies_task = overwatch_handle
            .runtime()
            .spawn(handle_validator_events_stream(events_streams));
        let (sampling_broadcast_sender, sampling_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let (verifying_broadcast_sender, verifying_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        Self {
            task,
            replies_task,
            sampling_request_channel,
            sampling_broadcast_sender,
            sampling_broadcast_receiver,
            verifying_broadcast_sender,
            verifying_broadcast_receiver,
            _membership: Default::default(),
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            DaNetworkMessage::RequestSample {
                subnetwork_id,
                blob_id,
            } => {
                self.handle_sample_request(subnetwork_id, blob_id).await;
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
                    .map(Self::NetworkEvent::Verifying),
            ),
        }
    }
}

async fn handle_validator_events_stream(events_streams: ValidatorEventsStream) {
    let ValidatorEventsStream {
        mut sampling_events_receiver,
    } = events_streams;
    #[allow(clippy::never_loop)]
    loop {
        tokio::select! {
            Some(sampling_event) = StreamExt::next(&mut sampling_events_receiver) => {
                unimplemented!()
            }
        }
    }
}

use crate::backends::libp2p::common::{
    dial_validator_subnetwork_peers, handle_sample_request, handle_validator_events_stream,
    DaNetworkBackendSettings, SamplingEvent, BROADCAST_CHANNEL_SIZE,
};
use crate::backends::NetworkBackend;
use futures::{Stream, StreamExt};
use kzgrs_backend::common::blob::DaBlob;
use libp2p::PeerId;
use log::error;
use nomos_core::da::BlobId;
use nomos_da_network_core::protocols::dispersal::executor::behaviour::DispersalExecutorEvent;
use nomos_da_network_core::swarm::executor::ExecutorSwarm;
use nomos_da_network_core::SubnetworkId;
use nomos_libp2p::ed25519;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use overwatch_rs::services::state::NoState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};

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
/// It forwards network messages to the corresponding subscription channels/streams
pub struct DaNetworkExecutorBackend<Membership>
where
    Membership: MembershipHandler,
{
    // TODO: this join handles should be cancelable tasks. We should add an stop method for
    // the `NetworkBackend` trait so if the service is stopped the backend can gracefully handle open
    // sub-tasks as well.
    #[allow(dead_code)]
    task: JoinHandle<()>,
    #[allow(dead_code)]
    verifier_replies_task: JoinHandle<()>,
    #[allow(dead_code)]
    executor_replies_task: JoinHandle<()>,
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
        );
        let address = config.validator_settings.listening_address.clone();
        // put swarm to listen at the specified configuration address
        executor_swarm
            .protocol_swarm_mut()
            .listen_on(address.clone())
            .unwrap_or_else(|e| {
                panic!("Error listening on DA network with address {address}: {e}")
            });
        // Dial peers in the same subnetworks (Node might participate in multiple).
        let local_peer_id = *executor_swarm.local_peer_id();

        let validator_subnetworks_connected_peers = dial_validator_subnetwork_peers(
            &config.validator_settings.membership,
            &config.validator_settings.addresses,
            executor_swarm.protocol_swarm_mut(),
            local_peer_id,
        );

        let dispersal_peers = dial_dispersal_peers(
            &mut executor_swarm,
            &config,
            &validator_subnetworks_connected_peers,
        );

        let sampling_request_channel = executor_swarm.sample_request_channel();

        let dispersal_blobs_sender = executor_swarm.dispersal_blobs_channel();
        let executor_open_stream_sender = executor_swarm.dispersal_open_stream_sender();

        let task = overwatch_handle.runtime().spawn(executor_swarm.run());

        // open streams to dispersal peers
        for peer_id in dispersal_peers.iter() {
            executor_open_stream_sender.send(*peer_id).unwrap();
        }

        let (sampling_broadcast_sender, sampling_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let (verifying_broadcast_sender, verifying_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let (dispersal_broadcast_sender, dispersal_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let verifier_replies_task =
            overwatch_handle
                .runtime()
                .spawn(handle_validator_events_stream(
                    executor_events_stream.validator_events_stream,
                    sampling_broadcast_sender,
                    verifying_broadcast_sender,
                ));
        let executor_replies_task =
            overwatch_handle
                .runtime()
                .spawn(handle_executor_dispersal_events_stream(
                    executor_events_stream.dispersal_events_receiver,
                    dispersal_broadcast_sender,
                ));

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

    async fn process(&self, msg: Self::Message) {
        match msg {
            ExecutorDaNetworkMessage::RequestSample {
                subnetwork_id,
                blob_id,
            } => {
                handle_sample_request(&self.sampling_request_channel, subnetwork_id, blob_id).await;
            }
            ExecutorDaNetworkMessage::RequestDispersal {
                subnetwork_id,
                da_blob,
            } => {
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

fn dial_dispersal_peers<Membership>(
    executor_swarm: &mut ExecutorSwarm<Membership>,
    config: &DaNetworkExecutorBackendSettings<Membership>,
    validator_connected_peers: &HashSet<PeerId>,
) -> HashSet<PeerId>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    let mut connected_peers = HashSet::new();

    let local_peer_id = *executor_swarm.local_peer_id();
    let membership = &config.validator_settings.membership;

    // filter out which subnetworks we already have connections with
    let connected_subnetworks: HashSet<SubnetworkId> = (0..config.num_subnets as u32)
        .filter(|subnetwork_id| {
            !membership
                .members_of(subnetwork_id)
                .is_disjoint(validator_connected_peers)
        })
        .collect();

    let already_connected_peers: HashSet<PeerId> = membership
        .members()
        .intersection(validator_connected_peers)
        .copied()
        .collect();

    // select dispersal peers from the subnetworks we are not connected yet
    let selected_dispersal_peers = select_dispersal_peers(
        &local_peer_id,
        config,
        &connected_subnetworks,
        //
        &HashSet::new(),
    );

    for peer_id in selected_dispersal_peers {
        let addr = config
            .validator_settings
            .addresses
            .get(&peer_id)
            .expect("Peer address should be in the list")
            .clone();

        executor_swarm
            .dial(addr)
            .expect("Should schedule the dials");

        connected_peers.insert(peer_id);
    }

    // add peers from the subnetwork we are connected with
    connected_peers
        .union(&already_connected_peers)
        .copied()
        .collect()
}

/// Use selection as per the base [specification](https://www.notion.so/NomosDA-Network-Specification-c6664294d630470ba20aefb21a218f8c?pvs=4#10e8f96fb65c803f9ed9d5a91df3ac83)
fn select_dispersal_peers<Membership>(
    local_peer_id: &PeerId,
    config: &DaNetworkExecutorBackendSettings<Membership>,
    filtered_subnetworks: &HashSet<SubnetworkId>,
    filtered_peers: &HashSet<PeerId>,
) -> HashSet<PeerId>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    let membership = &config.validator_settings.membership;
    (0..config.num_subnets as u32)
        .filter(|subnetwork_id| !filtered_subnetworks.contains(subnetwork_id))
        .filter_map(|subnetwork_id| {
            membership
                .members_of(&subnetwork_id)
                .difference(filtered_peers)
                .find(|&id| id != local_peer_id)
                .copied()
        })
        .collect()
}

// std
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
// crates
use futures::{Stream, StreamExt};
use kzgrs_backend::common::blob::DaBlob;
use libp2p::{Multiaddr, PeerId};
use log::error;
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::backends::NetworkBackend;
use nomos_libp2p::{ed25519, secret_key_serde};
use overwatch_rs::overwatch::handle::OverwatchHandle;
use overwatch_rs::services::state::NoState;
use serde::{Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::broadcast;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::{BroadcastStream, UnboundedReceiverStream};
// internal
use super::swarm::{DispersalEvent, ExecutorSwarm};

const BROADCAST_CHANNEL_SIZE: usize = 128;

/// Message that the backend replies to
#[derive(Debug)]
pub enum Command {
    /// Disperse a blob to a subnetwork.
    Disperse {
        subnetwork_id: SubnetworkId,
        blob: DaBlob,
    },
}

/// DA network backend for nomos cli as an executor.
/// Internally uses a libp2p swarm composed of the [`ExecutorBehaviour`]
/// It forwards network messages to the corresponding subscription channels/streams
pub struct ExecutorBackend<Membership> {
    // TODO: this join handles should be cancelable tasks. We should add an stop method for
    // the `NetworkBackend` trait so if the service is stopped the backend can gracefully handle open
    // sub-tasks as well.
    #[allow(dead_code)]
    task: JoinHandle<()>,
    #[allow(dead_code)]
    replies_task: JoinHandle<()>,
    dispersal_request_sender: UnboundedSender<(SubnetworkId, DaBlob)>,
    dispersal_broadcast_receiver: broadcast::Receiver<DispersalEvent>,
    _membership: PhantomData<Membership>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutorBackendSettings<Membership> {
    // Identification Secp256k1 private key in Hex format (`0x123...abc`). Default random.
    #[serde(with = "secret_key_serde", default = "ed25519::SecretKey::generate")]
    pub node_key: ed25519::SecretKey,
    /// Membership of DA network PoV set
    pub membership: Membership,
    pub node_addrs: HashMap<PeerId, Multiaddr>,
    pub num_subnets: u16,
}

impl<Membership> ExecutorBackend<Membership> {
    /// Send the dispersal request to the underlying dispersal behaviour
    async fn handle_dispersal_request(&self, subnetwork_id: SubnetworkId, blob: DaBlob) {
        if let Err(SendError((subnetwork_id, blob_id))) =
            self.dispersal_request_sender.send((subnetwork_id, blob))
        {
            error!(
                "Error requesting sample for subnetwork id : {subnetwork_id}, blob_id: {blob_id:?}"
            );
        }
    }
}

#[async_trait::async_trait]
impl<Membership> NetworkBackend for ExecutorBackend<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type Settings = ExecutorBackendSettings<Membership>;
    type State = NoState<Self::Settings>;
    type Message = Command;
    type EventKind = ();
    type NetworkEvent = DispersalEvent;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let (dispersal_events_sender, dispersal_events_receiver) = unbounded_channel();

        let keypair =
            libp2p::identity::Keypair::from(ed25519::Keypair::from(config.node_key.clone()));
        let mut executor_swarm =
            ExecutorSwarm::new(keypair, config.membership.clone(), dispersal_events_sender);
        let dispersal_request_sender = executor_swarm.blobs_sender();

        let mut connected_peers = HashSet::new();

        let local_peer_id = *executor_swarm.local_peer_id();
        for subnetwork_id in 0..config.num_subnets {
            // Connect to one peer in a subnet.
            let mut members = config.membership.members_of(&(subnetwork_id as u32));
            members.remove(&local_peer_id);
            let peer_id = *members
                .iter()
                .next()
                .expect("Subnet should have at least one node which is not the nomos_cli");

            let addr = config
                .node_addrs
                .get(&peer_id)
                .expect("Peer address should be in the list");

            executor_swarm
                .dial(addr.clone())
                .expect("Should schedule the dials");

            connected_peers.insert(peer_id);
        }

        let executor_open_stream_sender = executor_swarm.open_stream_sender();

        let task = overwatch_handle
            .runtime()
            .spawn(async move { executor_swarm.run().await });

        for peer_id in connected_peers.iter() {
            executor_open_stream_sender.send(*peer_id).unwrap();
        }

        let (dispersal_broadcast_sender, dispersal_broadcast_receiver) =
            broadcast::channel(BROADCAST_CHANNEL_SIZE);
        let dispersal_events_receiver = UnboundedReceiverStream::new(dispersal_events_receiver);

        let replies_task = overwatch_handle
            .runtime()
            .spawn(handle_dispersal_events_stream(
                dispersal_events_receiver,
                dispersal_broadcast_sender,
            ));

        Self {
            task,
            replies_task,
            dispersal_request_sender,
            dispersal_broadcast_receiver,
            _membership: Default::default(),
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            Command::Disperse {
                subnetwork_id,
                blob,
            } => {
                self.handle_dispersal_request(subnetwork_id, blob).await;
            }
        }
    }

    async fn subscribe(
        &mut self,
        _event: Self::EventKind,
    ) -> Pin<Box<dyn Stream<Item = Self::NetworkEvent> + Send>> {
        Box::pin(
            BroadcastStream::new(self.dispersal_broadcast_receiver.resubscribe())
                .filter_map(|event| async { event.ok() }),
        )
    }
}

/// Task that handles forwarding of events to the subscriptions channels/stream
async fn handle_dispersal_events_stream(
    mut events_stream: UnboundedReceiverStream<DispersalEvent>,
    dispersal_broadcast_sender: broadcast::Sender<DispersalEvent>,
) {
    while let Some(dispersal_event) = events_stream.next().await {
        if let Err(e) = dispersal_broadcast_sender.send(dispersal_event) {
            error!("Error in internal broadcast of dispersal event: {e:?}");
        }
    }
}

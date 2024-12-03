// std
use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll};
// crates
use either::Either;
use futures::future::BoxFuture;
use futures::stream::{BoxStream, FuturesUnordered};
use futures::{AsyncWriteExt, FutureExt, StreamExt};
use libp2p::core::Endpoint;
use libp2p::swarm::behaviour::{ConnectionClosed, ConnectionEstablished};
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::{Control, OpenStreamError};
use opentelemetry::{metrics::Counter, KeyValue};
use rand::prelude::IteratorRandom;
use rand::SeedableRng;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::error;
// internal
use crate::address_book::AddressBook;
use crate::protocol::DISPERSAL_PROTOCOL;
use crate::protocols::clone_deserialize_error;
use crate::SubnetworkId;
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::BlobId;
use nomos_da_messages::common::Blob;
use nomos_da_messages::dispersal::dispersal_res::MessageType;
use nomos_da_messages::dispersal::{DispersalErr, DispersalReq, DispersalRes};
use nomos_da_messages::{pack_message, unpack_from_reader};
use subnetworks_assignations::MembershipHandler;

#[derive(Debug, Error)]
pub enum DispersalError {
    #[error("Stream disconnected: {error}")]
    Io {
        error: std::io::Error,
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
    },
    #[error("Could not serialized: {error}")]
    Serialization {
        error: bincode::Error,
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
    },
    #[error("Dispersal response error: {error:?}")]
    Protocol {
        subnetwork_id: SubnetworkId,
        error: DispersalErr,
    },
    #[error("Error dialing peer [{peer_id}]: {error}")]
    OpenStreamError {
        peer_id: PeerId,
        error: OpenStreamError,
    },
}

impl DispersalError {
    pub fn blob_id(&self) -> Option<BlobId> {
        match self {
            DispersalError::Io { blob_id, .. } => Some(*blob_id),
            DispersalError::Serialization { blob_id, .. } => Some(*blob_id),
            DispersalError::Protocol {
                error: DispersalErr { blob_id, .. },
                ..
            } => Some(blob_id.clone().try_into().unwrap()),
            DispersalError::OpenStreamError { .. } => None,
        }
    }

    pub fn subnetwork_id(&self) -> Option<SubnetworkId> {
        match self {
            DispersalError::Io { subnetwork_id, .. } => Some(*subnetwork_id),
            DispersalError::Serialization { subnetwork_id, .. } => Some(*subnetwork_id),
            DispersalError::Protocol { subnetwork_id, .. } => Some(*subnetwork_id),
            DispersalError::OpenStreamError { .. } => None,
        }
    }
}

impl Clone for DispersalError {
    fn clone(&self) -> Self {
        match self {
            DispersalError::Io {
                error,
                blob_id,
                subnetwork_id,
            } => DispersalError::Io {
                error: std::io::Error::new(error.kind(), error.to_string()),
                blob_id: *blob_id,
                subnetwork_id: *subnetwork_id,
            },
            DispersalError::Serialization {
                error,
                blob_id,
                subnetwork_id,
            } => DispersalError::Serialization {
                error: clone_deserialize_error(error),
                blob_id: *blob_id,
                subnetwork_id: *subnetwork_id,
            },
            DispersalError::Protocol {
                subnetwork_id,
                error,
            } => DispersalError::Protocol {
                subnetwork_id: *subnetwork_id,
                error: error.clone(),
            },
            DispersalError::OpenStreamError { peer_id, error } => DispersalError::OpenStreamError {
                peer_id: *peer_id,
                error: match error {
                    OpenStreamError::UnsupportedProtocol(protocol) => {
                        OpenStreamError::UnsupportedProtocol(protocol.clone())
                    }
                    OpenStreamError::Io(error) => {
                        OpenStreamError::Io(std::io::Error::new(error.kind(), error.to_string()))
                    }
                    err => OpenStreamError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        err.to_string(),
                    )),
                },
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum DispersalExecutorEvent {
    /// A blob successfully arrived its destination
    DispersalSuccess {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
    },
    /// Something went wrong delivering the blob
    DispersalError { error: DispersalError },
}

impl DispersalExecutorEvent {
    const EVENT_NAME: &'static str = "dispersal_executor";

    pub fn log_with_counter(&self, counter: &mut Counter<u64>) {
        counter.add(1, &[KeyValue::new("event", Self::EVENT_NAME)]);
    }
}

struct DispersalStream {
    stream: Stream,
    peer_id: PeerId,
}

type StreamHandlerFutureSuccess = (BlobId, SubnetworkId, DispersalRes, DispersalStream);
type StreamHandlerFuture = BoxFuture<'static, Result<StreamHandlerFutureSuccess, DispersalError>>;

/// Executor dispersal protocol
/// Do not handle incoming connections, just accepts outgoing ones.
/// It takes care of sending blobs to different subnetworks.
/// Bubbles up events with the success or error when dispersing
pub struct DispersalExecutorBehaviour<Membership: MembershipHandler> {
    /// Self id
    local_peer_id: PeerId,
    /// Underlying stream behaviour
    stream_behaviour: libp2p_stream::Behaviour,
    /// Pending running tasks (one task per stream)
    tasks: FuturesUnordered<StreamHandlerFuture>,
    /// Streams which didn't have any pending task
    idle_streams: HashMap<PeerId, DispersalStream>,
    /// Subnetworks membership information
    membership: Membership,
    /// Addresses of known peers in the DA network
    addresses: AddressBook,
    /// Pending blobs that need to be dispersed by PeerId
    to_disperse: HashMap<PeerId, VecDeque<(Membership::NetworkId, DaBlob)>>,
    /// Pending blobs from disconnected networks
    disconnected_pending_blobs: HashMap<Membership::NetworkId, VecDeque<DaBlob>>,
    /// Already connected peers connection Ids
    connected_peers: HashMap<PeerId, ConnectionId>,
    /// Subnetwork working streams
    subnetwork_open_streams: HashSet<SubnetworkId>,
    /// Sender hook of peers to open streams channel
    pending_out_streams_sender: UnboundedSender<PeerId>,
    /// Pending to open streams
    pending_out_streams: BoxStream<'static, Result<DispersalStream, DispersalError>>,
    /// Dispersal hook of pending blobs channel
    pending_blobs_sender: UnboundedSender<(Membership::NetworkId, DaBlob)>,
    /// Pending blobs stream
    pending_blobs_stream: BoxStream<'static, (Membership::NetworkId, DaBlob)>,
}

impl<Membership> DispersalExecutorBehaviour<Membership>
where
    Membership: MembershipHandler + 'static,
    Membership::NetworkId: Send,
{
    pub fn new(local_peer_id: PeerId, membership: Membership, addresses: AddressBook) -> Self {
        let stream_behaviour = libp2p_stream::Behaviour::new();
        let tasks = FuturesUnordered::new();
        let to_disperse = HashMap::new();
        let connected_peers = HashMap::new();
        let subnetwork_open_streams = HashSet::new();
        let idle_streams = HashMap::new();
        let (pending_out_streams_sender, receiver) = mpsc::unbounded_channel();
        let control = stream_behaviour.new_control();
        let pending_out_streams = UnboundedReceiverStream::new(receiver)
            .zip(futures::stream::repeat(control))
            .then(|(peer_id, control)| Self::open_stream(peer_id, control))
            .boxed();

        let (pending_blobs_sender, receiver) = mpsc::unbounded_channel();
        let pending_blobs_stream = UnboundedReceiverStream::new(receiver).boxed();
        let disconnected_pending_blobs = HashMap::new();

        Self {
            local_peer_id,
            stream_behaviour,
            tasks,
            membership,
            addresses,
            to_disperse,
            disconnected_pending_blobs,
            connected_peers,
            subnetwork_open_streams,
            idle_streams,
            pending_out_streams_sender,
            pending_out_streams,
            pending_blobs_sender,
            pending_blobs_stream,
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        self.membership = membership;
    }

    /// Open a new stream from the underlying control to the provided peer
    async fn open_stream(
        peer_id: PeerId,
        mut control: Control,
    ) -> Result<DispersalStream, DispersalError> {
        let stream = control
            .open_stream(peer_id, DISPERSAL_PROTOCOL)
            .await
            .map_err(|error| DispersalError::OpenStreamError { peer_id, error })?;
        Ok(DispersalStream { stream, peer_id })
    }

    /// Get a hook to the sender channel of open stream events
    pub fn open_stream_sender(&self) -> UnboundedSender<PeerId> {
        self.pending_out_streams_sender.clone()
    }

    /// Get a hook to the sender channel of the blobs dispersal events
    pub fn blobs_sender(&self) -> UnboundedSender<(Membership::NetworkId, DaBlob)> {
        self.pending_blobs_sender.clone()
    }

    /// Task for handling streams, one message at a time
    /// Writes the blob to the stream and waits for an acknowledgment response
    async fn stream_disperse(
        mut stream: DispersalStream,
        message: DaBlob,
        subnetwork_id: SubnetworkId,
    ) -> Result<StreamHandlerFutureSuccess, DispersalError> {
        let blob_id = message.id();
        let blob = bincode::serialize(&message).map_err(|error| DispersalError::Serialization {
            error,
            blob_id: blob_id.clone().try_into().unwrap(),
            subnetwork_id,
        })?;
        let message = DispersalReq {
            blob: Some(Blob {
                blob_id: blob_id.clone(),
                data: blob,
            }),
            subnetwork_id,
        };
        stream
            .stream
            .write_all(&pack_message(&message).map_err(|error| DispersalError::Io {
                error,
                blob_id: blob_id.clone().try_into().unwrap(),
                subnetwork_id,
            })?)
            .await
            .map_err(|error| DispersalError::Io {
                error,
                blob_id: blob_id.clone().try_into().unwrap(),
                subnetwork_id,
            })?;
        stream
            .stream
            .flush()
            .await
            .map_err(|error| DispersalError::Io {
                error,
                blob_id: blob_id.clone().try_into().unwrap(),
                subnetwork_id,
            })?;
        let response: DispersalRes =
            unpack_from_reader(&mut stream.stream)
                .await
                .map_err(|error| DispersalError::Io {
                    error,
                    blob_id: blob_id.clone().try_into().unwrap(),
                    subnetwork_id,
                })?;
        // Safety: blob_id should always be a 32bytes hash, currently is abstracted into a `Vec<u8>`
        // but probably we should have a `[u8; 32]` wrapped in a custom type `BlobId`
        // TODO: use blob_id when changing types to [u8; 32]
        Ok((blob_id.try_into().unwrap(), subnetwork_id, response, stream))
    }

    /// Run when a stream gets free, if there is a pending task for the stream it will get scheduled to run
    /// otherwise it is parked as idle.
    fn handle_stream(
        tasks: &mut FuturesUnordered<StreamHandlerFuture>,
        to_disperse: &mut HashMap<PeerId, VecDeque<(SubnetworkId, DaBlob)>>,
        idle_streams: &mut HashMap<PeerId, DispersalStream>,
        stream: DispersalStream,
    ) {
        if let Some((subnetwork_id, next_request)) =
            Self::next_request(&stream.peer_id, to_disperse)
        {
            let fut = Self::stream_disperse(stream, next_request, subnetwork_id).boxed();
            tasks.push(fut);
        } else {
            // There is no pending request, so just idle the stream
            idle_streams.insert(stream.peer_id, stream);
        };
    }

    /// Get a pending request if its available
    fn next_request(
        peer_id: &PeerId,
        to_disperse: &mut HashMap<PeerId, VecDeque<(SubnetworkId, DaBlob)>>,
    ) -> Option<(SubnetworkId, DaBlob)> {
        to_disperse
            .get_mut(peer_id)
            .and_then(|queue| queue.pop_front())
    }
}

impl<Membership: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static>
    DispersalExecutorBehaviour<Membership>
{
    /// Schedule a new task for sending the blob, if stream is not available queue messages for later
    /// processing.
    fn disperse_blob(
        tasks: &mut FuturesUnordered<StreamHandlerFuture>,
        idle_streams: &mut HashMap<Membership::Id, DispersalStream>,
        membership: &Membership,
        connected_peers: &HashMap<PeerId, ConnectionId>,
        to_disperse: &mut HashMap<PeerId, VecDeque<(Membership::NetworkId, DaBlob)>>,
        subnetwork_id: SubnetworkId,
        blob: DaBlob,
    ) {
        let members = membership.members_of(&subnetwork_id);
        let peers = members
            .iter()
            .filter(|peer_id| connected_peers.contains_key(peer_id));

        // We may be connected to more than a single node. Usually will be one, but that is an
        // internal decision of the executor itself.
        for peer in peers {
            if let Some(stream) = idle_streams.remove(peer) {
                // push a task if the stream is immediately available
                let fut = Self::stream_disperse(stream, blob.clone(), subnetwork_id).boxed();
                tasks.push(fut);
            } else {
                // otherwise queue the blob
                to_disperse
                    .entry(*peer)
                    .or_default()
                    .push_back((subnetwork_id, blob.clone()));
            }
        }
    }

    fn reschedule_blobs_for_peer_stream(
        stream: &DispersalStream,
        membership: &Membership,
        to_disperse: &mut HashMap<PeerId, VecDeque<(SubnetworkId, DaBlob)>>,
        disconnected_pending_blobs: &mut HashMap<SubnetworkId, VecDeque<DaBlob>>,
    ) {
        let peer_id = stream.peer_id;
        let subnetworks = membership.membership(&peer_id);
        let entry = to_disperse.entry(peer_id).or_default();
        for subnetwork in subnetworks {
            if let Some(blobs) = disconnected_pending_blobs.remove(&subnetwork) {
                entry.extend(blobs.into_iter().map(|blob| (subnetwork, blob)));
            }
        }
    }

    fn filter_peers_for_subnetworks<'s>(
        &'s self,
        peer_id: PeerId,
        subnetworks: impl Iterator<Item = SubnetworkId> + 's,
    ) -> impl Iterator<Item = HashSet<PeerId>> + 's {
        subnetworks.map(move |subnetwork_id| {
            self.membership
                .members_of(&subnetwork_id)
                .iter()
                .filter(|&&peer| peer != peer_id && peer != self.local_peer_id)
                .copied()
                .collect::<HashSet<_>>()
        })
    }

    fn find_subnetworks_candidates_excluding_peer(
        &self,
        peer_id: PeerId,
        subnetworks: &HashSet<SubnetworkId>,
    ) -> HashSet<PeerId> {
        let mut peers: HashSet<PeerId> = self
            .filter_peers_for_subnetworks(peer_id, subnetworks.iter().copied())
            .reduce(|h1, h2| h1.intersection(&h2).copied().collect())
            .unwrap_or_default();
        // we didn't find a single shared peer for all subnetworks, so we take the smallest subset
        if peers.is_empty() {
            peers = self
                .filter_peers_for_subnetworks(peer_id, subnetworks.iter().copied())
                .reduce(|h1, h2| h1.union(&h2).copied().collect())
                .unwrap_or_default();
        }
        peers
    }
    fn open_streams_for_disconnected_subnetworks_selected_peer(&mut self, peer_id: PeerId) {
        let subnetworks = self.membership.membership(&peer_id);
        // open stream will result in dialing if we are not yet connected to the peer
        for peer in self.find_subnetworks_candidates_excluding_peer(peer_id, &subnetworks) {
            if let Err(e) = self.pending_out_streams_sender.send(peer) {
                error!("Error requesting stream for peer {peer_id}: {e}");
            }
        }
    }

    fn prune_blobs_for_peer(&mut self, peer_id: PeerId) -> VecDeque<(SubnetworkId, DaBlob)> {
        self.to_disperse.remove(&peer_id).unwrap_or_default()
    }

    fn recover_blobs_for_disconnected_subnetworks(&mut self, peer_id: PeerId) {
        // push missing blobs into pending ones
        let disconnected_pending_blobs = self.prune_blobs_for_peer(peer_id);
        for (subnetwork_id, blob) in disconnected_pending_blobs {
            self.disconnected_pending_blobs
                .entry(subnetwork_id)
                .or_default()
                .push_back(blob);
        }
    }

    fn try_ensure_stream_from_missing_subnetwork(
        local_peer_id: &PeerId,
        pending_out_streams_sender: &mut UnboundedSender<PeerId>,
        membership: &Membership,
        subnetwork_id: &SubnetworkId,
    ) {
        let mut rng = rand_chacha::ChaCha20Rng::from_entropy();
        // chose a random peer that is not us
        let peer = membership
            .members_of(subnetwork_id)
            .iter()
            .filter(|&peer| peer != local_peer_id)
            .choose(&mut rng)
            .copied();
        // if we have any, try to connect
        if let Some(peer) = peer {
            if let Err(e) = pending_out_streams_sender.send(peer) {
                error!("Error requesting stream for peer {peer}: {e}");
            }
        }
    }

    fn handle_connection_established(&mut self, peer_id: PeerId, connection_id: ConnectionId) {
        self.connected_peers.insert(peer_id, connection_id);
    }

    fn handle_connection_closed(&mut self, peer_id: PeerId) {
        let peer_subnetworks = self.membership.membership(&peer_id);
        self.subnetwork_open_streams
            .retain(|subnetwork_id| !peer_subnetworks.contains(subnetwork_id));
        if self.connected_peers.remove(&peer_id).is_some() {
            // mangle pending blobs for disconnected subnetworks from peer
            self.recover_blobs_for_disconnected_subnetworks(peer_id);
            self.open_streams_for_disconnected_subnetworks_selected_peer(peer_id);
        }
    }
}

impl<M: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static> NetworkBehaviour
    for DispersalExecutorBehaviour<M>
{
    type ConnectionHandler = Either<
        <libp2p_stream::Behaviour as NetworkBehaviour>::ConnectionHandler,
        libp2p::swarm::dummy::ConnectionHandler,
    >;
    type ToSwarm = DispersalExecutorEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.stream_behaviour
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
            .map(Either::Left)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.stream_behaviour.on_swarm_event(event);
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                self.handle_connection_established(peer_id, connection_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, .. }) => {
                self.handle_connection_closed(peer_id);
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let Either::Left(event) = event;
        self.stream_behaviour
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        let Self {
            local_peer_id,
            tasks,
            to_disperse,
            disconnected_pending_blobs,
            idle_streams,
            pending_out_streams,
            pending_out_streams_sender,
            pending_blobs_stream,
            membership,
            addresses,
            connected_peers,
            subnetwork_open_streams,
            ..
        } = self;
        // poll pending tasks
        if let Poll::Ready(Some(future_result)) = tasks.poll_next_unpin(cx) {
            match future_result {
                Ok((blob_id, subnetwork_id, dispersal_response, stream)) => {
                    // handle the free stream then return the success
                    Self::handle_stream(tasks, to_disperse, idle_streams, stream);
                    // return an error if there was an error on the other side of the wire
                    if let DispersalRes {
                        message_type: Some(MessageType::Err(error)),
                    } = dispersal_response
                    {
                        return Poll::Ready(ToSwarm::GenerateEvent(
                            DispersalExecutorEvent::DispersalError {
                                error: DispersalError::Protocol {
                                    subnetwork_id,
                                    error,
                                },
                            },
                        ));
                    }
                    return Poll::Ready(ToSwarm::GenerateEvent(
                        DispersalExecutorEvent::DispersalSuccess {
                            blob_id,
                            subnetwork_id,
                        },
                    ));
                }
                // Something went up on our side of the wire, bubble it up
                Err(error) => {
                    return Poll::Ready(ToSwarm::GenerateEvent(
                        DispersalExecutorEvent::DispersalError { error },
                    ))
                }
            }
        }
        // poll pending blobs
        if let Poll::Ready(Some((subnetwork_id, blob))) = pending_blobs_stream.poll_next_unpin(cx) {
            if subnetwork_open_streams.contains(&subnetwork_id) {
                Self::disperse_blob(
                    tasks,
                    idle_streams,
                    membership,
                    connected_peers,
                    to_disperse,
                    subnetwork_id,
                    blob,
                );
            } else {
                let entry = disconnected_pending_blobs.entry(subnetwork_id).or_default();
                entry.push_back(blob);
                Self::try_ensure_stream_from_missing_subnetwork(
                    local_peer_id,
                    pending_out_streams_sender,
                    membership,
                    &subnetwork_id,
                );
            }
        }
        // poll pending streams
        if let Poll::Ready(Some(res)) = pending_out_streams.poll_next_unpin(cx) {
            match res {
                Ok(stream) => {
                    subnetwork_open_streams.extend(membership.membership(&stream.peer_id));
                    Self::reschedule_blobs_for_peer_stream(
                        &stream,
                        membership,
                        to_disperse,
                        disconnected_pending_blobs,
                    );
                    Self::handle_stream(tasks, to_disperse, idle_streams, stream);
                }
                Err(error) => {
                    return Poll::Ready(ToSwarm::GenerateEvent(
                        DispersalExecutorEvent::DispersalError { error },
                    ));
                }
            }
        }
        // Deal with connection as the underlying behaviour would do
        match self.stream_behaviour.poll(cx) {
            Poll::Ready(ToSwarm::Dial { mut opts }) => {
                // attach known peer address if possible
                if let Some(address) = opts
                    .get_peer_id()
                    .and_then(|peer_id: PeerId| addresses.get_address(&peer_id))
                {
                    opts = DialOpts::peer_id(opts.get_peer_id().unwrap())
                        .addresses(vec![address.clone()])
                        .build();
                }
                Poll::Ready(ToSwarm::Dial { opts })
            }
            Poll::Pending => {
                // TODO: probably must be smarter when to wake this
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            _ => unreachable!(),
        }
    }
}

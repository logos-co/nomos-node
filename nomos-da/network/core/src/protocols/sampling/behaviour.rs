// std
use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll};
// crates
use either::Either;
use futures::channel::oneshot;
use futures::channel::oneshot::{Canceled, Receiver, Sender};
use futures::future::BoxFuture;
use futures::stream::{BoxStream, FuturesUnordered};
use futures::{AsyncWriteExt, FutureExt, StreamExt, TryFutureExt};
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::common::ColumnIndex;
use libp2p::core::Endpoint;
use libp2p::swarm::dial_opts::DialOpts;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::{Control, IncomingStreams, OpenStreamError};
use nomos_core::da::BlobId;
use nomos_core::wire;
use nomos_da_messages::packing::{pack_to_writer, unpack_from_reader};
use nomos_da_messages::{common, sampling};
use subnetworks_assignations::MembershipHandler;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::error;
// internal
use crate::address_book::AddressBook;
use crate::protocol::SAMPLING_PROTOCOL;
use crate::SubnetworkId;

#[derive(Debug, Error)]
pub enum SamplingError {
    #[error("Stream disconnected: {error}")]
    Io {
        peer_id: PeerId,
        error: std::io::Error,
    },
    #[error("Dispersal response error: {error:?}")]
    Protocol {
        subnetwork_id: SubnetworkId,
        peer_id: PeerId,
        error: sampling::SampleError,
    },
    #[error("Error opening stream [{peer_id}]: {error}")]
    OpenStream {
        peer_id: PeerId,
        error: OpenStreamError,
    },
    #[error("Unable to deserialize blob response: {error}")]
    Deserialize {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
        peer_id: PeerId,
        error: wire::Error,
    },
    #[error("Error sending request: {request:?}")]
    RequestChannel {
        request: BehaviourSampleReq,
        peer_id: PeerId,
    },
    #[error("Malformed blob id: {blob_id:?}")]
    InvalidBlobId { peer_id: PeerId, blob_id: Vec<u8> },
    #[error("Blob not found: {blob_id:?}")]
    BlobNotFound { peer_id: PeerId, blob_id: Vec<u8> },
    #[error("Canceled response: {error}")]
    ResponseChannel { error: Canceled, peer_id: PeerId },
}

impl SamplingError {
    pub fn peer_id(&self) -> &PeerId {
        match self {
            SamplingError::Io { peer_id, .. } => peer_id,
            SamplingError::Protocol { peer_id, .. } => peer_id,
            SamplingError::OpenStream { peer_id, .. } => peer_id,
            SamplingError::Deserialize { peer_id, .. } => peer_id,
            SamplingError::RequestChannel { peer_id, .. } => peer_id,
            SamplingError::ResponseChannel { peer_id, .. } => peer_id,
            SamplingError::InvalidBlobId { peer_id, .. } => peer_id,
            SamplingError::BlobNotFound { peer_id, .. } => peer_id,
        }
    }

    pub fn blob_id(&self) -> Option<&BlobId> {
        match self {
            SamplingError::Deserialize { blob_id, .. } => Some(blob_id),
            _ => None,
        }
    }
}

impl Clone for SamplingError {
    fn clone(&self) -> Self {
        match self {
            SamplingError::Io { peer_id, error } => SamplingError::Io {
                peer_id: *peer_id,
                error: std::io::Error::new(error.kind(), error.to_string()),
            },
            SamplingError::Protocol {
                subnetwork_id,
                peer_id,
                error,
            } => SamplingError::Protocol {
                subnetwork_id: *subnetwork_id,
                peer_id: *peer_id,
                error: error.clone(),
            },
            SamplingError::OpenStream { peer_id, error } => SamplingError::OpenStream {
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
            SamplingError::Deserialize {
                blob_id,
                subnetwork_id,
                peer_id,
                error,
            } => SamplingError::Deserialize {
                blob_id: *blob_id,
                subnetwork_id: *subnetwork_id,
                peer_id: *peer_id,
                error: error.clone(),
            },
            SamplingError::RequestChannel { request, peer_id } => SamplingError::RequestChannel {
                request: request.clone(),
                peer_id: *peer_id,
            },
            SamplingError::ResponseChannel { error, peer_id } => SamplingError::ResponseChannel {
                peer_id: *peer_id,
                error: *error,
            },
            SamplingError::InvalidBlobId { blob_id, peer_id } => SamplingError::InvalidBlobId {
                peer_id: *peer_id,
                blob_id: blob_id.clone(),
            },
            SamplingError::BlobNotFound { blob_id, peer_id } => SamplingError::BlobNotFound {
                peer_id: *peer_id,
                blob_id: blob_id.clone(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct BehaviourSampleReq {
    pub blob_id: BlobId,
    pub column_idx: ColumnIndex,
}

impl TryFrom<sampling::SampleRequest> for BehaviourSampleReq {
    type Error = Vec<u8>;

    fn try_from(req: sampling::SampleRequest) -> Result<Self, Self::Error> {
        let sampling::SampleRequest {
            blob_id,
            column_idx,
        } = req;

        Ok(Self {
            blob_id,
            column_idx,
        })
    }
}

#[derive(Debug)]
pub enum BehaviourSampleRes {
    SamplingSuccess {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
        blob: Box<DaBlob>,
    },
    SampleNotFound {
        blob_id: BlobId,
    },
}

impl From<BehaviourSampleRes> for sampling::SampleResponse {
    fn from(res: BehaviourSampleRes) -> Self {
        match res {
            BehaviourSampleRes::SamplingSuccess { blob, blob_id, .. } => {
                sampling::SampleResponse::Blob(common::Blob::new(blob_id, *blob))
            }
            BehaviourSampleRes::SampleNotFound { blob_id } => {
                sampling::SampleResponse::Error(sampling::SampleError::new(
                    blob_id,
                    sampling::SampleErrorType::NotFound,
                    "Sample not found",
                ))
            }
        }
    }
}

#[derive(Debug)]
pub enum SamplingEvent {
    /// A blob successfully arrived its destination
    SamplingSuccess {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
        blob: Box<DaBlob>,
    },
    IncomingSample {
        request_receiver: Receiver<BehaviourSampleReq>,
        response_sender: Sender<BehaviourSampleRes>,
    },
    SamplingError {
        error: SamplingError,
    },
}

/// Auxiliary struct that binds a stream with the corresponding `PeerId`
struct SampleStream {
    stream: Stream,
    peer_id: PeerId,
}

/// Auxiliary struct that binds where to send a request and the pair channel to listen for a response
struct ResponseChannel {
    request_sender: Sender<BehaviourSampleReq>,
    response_receiver: Receiver<BehaviourSampleRes>,
}

type StreamHandlerFutureSuccess = (BlobId, SubnetworkId, sampling::SampleResponse, SampleStream);
type OutgoingStreamHandlerFuture =
    BoxFuture<'static, Result<StreamHandlerFutureSuccess, SamplingError>>;
type IncomingStreamHandlerFuture = BoxFuture<'static, Result<SampleStream, SamplingError>>;
/// Executor sampling protocol
/// Takes care of sending and replying sampling requests
pub struct SamplingBehaviour<Membership: MembershipHandler> {
    /// Self peer id
    peer_id: PeerId,
    /// Addresses of known peers in the DA network
    addresses: AddressBook,
    /// Underlying stream behaviour
    stream_behaviour: libp2p_stream::Behaviour,
    /// Incoming sample request streams
    incoming_streams: IncomingStreams,
    /// Underlying stream control
    control: Control,
    /// Pending outgoing running tasks (one task per stream)
    outgoing_tasks: FuturesUnordered<OutgoingStreamHandlerFuture>,
    /// Pending incoming running tasks (one task per stream)
    incoming_tasks: FuturesUnordered<IncomingStreamHandlerFuture>,
    /// Subnetworks membership information
    membership: Membership,
    /// Pending blobs that need to be dispersed by PeerId
    to_sample: HashMap<PeerId, VecDeque<(Membership::NetworkId, BlobId)>>,
    /// Already connected peers connection Ids
    connected_peers: HashSet<PeerId>,
    /// Hook of pending samples channel
    samples_request_sender: UnboundedSender<(Membership::NetworkId, BlobId)>,
    /// Pending samples stream
    samples_request_stream: BoxStream<'static, (Membership::NetworkId, BlobId)>,
}

impl<Membership> SamplingBehaviour<Membership>
where
    Membership: MembershipHandler + 'static,
    Membership::NetworkId: Send,
{
    pub fn new(peer_id: PeerId, membership: Membership, addresses: AddressBook) -> Self {
        let stream_behaviour = libp2p_stream::Behaviour::new();
        let mut control = stream_behaviour.new_control();

        let incoming_streams = control
            .accept(SAMPLING_PROTOCOL)
            .expect("Just a single accept to protocol is valid");

        let outgoing_tasks = FuturesUnordered::new();
        let incoming_tasks = FuturesUnordered::new();

        let to_sample = HashMap::new();

        let (samples_request_sender, receiver) = mpsc::unbounded_channel();
        let samples_request_stream = UnboundedReceiverStream::new(receiver).boxed();
        let connected_peers = HashSet::new();
        Self {
            peer_id,
            addresses,
            stream_behaviour,
            incoming_streams,
            control,
            outgoing_tasks,
            incoming_tasks,
            membership,
            to_sample,
            connected_peers,
            samples_request_sender,
            samples_request_stream,
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        self.membership = membership;
    }

    /// Open a new stream from the underlying control to the provided peer
    async fn open_stream(
        peer_id: PeerId,
        mut control: Control,
    ) -> Result<SampleStream, SamplingError> {
        let stream = control
            .open_stream(peer_id, SAMPLING_PROTOCOL)
            .await
            .map_err(|error| SamplingError::OpenStream { peer_id, error })?;
        Ok(SampleStream { stream, peer_id })
    }

    /// Get a hook to the sender channel of the sample events
    pub fn sample_request_channel(&self) -> UnboundedSender<(Membership::NetworkId, BlobId)> {
        self.samples_request_sender.clone()
    }

    /// Task for handling streams, one message at a time
    /// Writes the request to the stream and waits for a response
    async fn stream_sample(
        mut stream: SampleStream,
        message: sampling::SampleRequest,
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
    ) -> Result<StreamHandlerFutureSuccess, SamplingError> {
        let into_sampling_error = |error: std::io::Error| -> SamplingError {
            SamplingError::Io {
                peer_id: stream.peer_id,
                error,
            }
        };
        pack_to_writer(&message, &mut stream.stream)
            .map_err(into_sampling_error)
            .await?;
        stream.stream.flush().await.map_err(into_sampling_error)?;
        let response: sampling::SampleResponse = unpack_from_reader(&mut stream.stream)
            .await
            .map_err(into_sampling_error)?;
        // Safety: blob_id should always be a 32bytes hash
        Ok((blob_id, subnetwork_id, response, stream))
    }

    /// Get a pending outgoing request if its available
    fn next_request(
        peer_id: &PeerId,
        to_sample: &mut HashMap<PeerId, VecDeque<(SubnetworkId, BlobId)>>,
    ) -> Option<(SubnetworkId, BlobId)> {
        to_sample
            .get_mut(peer_id)
            .and_then(|queue| queue.pop_front())
    }

    /// Handle outgoing stream
    /// Schedule a new task if its available or drop the stream if not
    fn handle_outgoing_stream(
        outgoing_tasks: &mut FuturesUnordered<OutgoingStreamHandlerFuture>,
        to_sample: &mut HashMap<PeerId, VecDeque<(SubnetworkId, BlobId)>>,
        connected_peers: &mut HashSet<PeerId>,
        mut stream: SampleStream,
    ) {
        let peer = stream.peer_id;

        // If there is a pending task schedule next one
        if let Some((subnetwork_id, blob_id)) = Self::next_request(&peer, to_sample) {
            let sample_request = sampling::SampleRequest::new(blob_id, subnetwork_id);
            outgoing_tasks
                .push(Self::stream_sample(stream, sample_request, subnetwork_id, blob_id).boxed());
        // if not pop stream from connected ones
        } else {
            tokio::task::spawn(async move {
                if let Err(error) = stream.stream.close().await {
                    error!("Error closing sampling stream: {error}");
                };
            });
            connected_peers.remove(&peer);
        }
    }

    /// Handler incoming streams
    /// Pull a request from the stream and replies if possible
    async fn handle_incoming_stream(
        mut stream: SampleStream,
        channel: ResponseChannel,
    ) -> Result<SampleStream, SamplingError> {
        let request: sampling::SampleRequest = unpack_from_reader(&mut stream.stream)
            .await
            .map_err(|error| SamplingError::Io {
                peer_id: stream.peer_id,
                error,
            })?;
        let request = BehaviourSampleReq::try_from(request).map_err(|blob_id| {
            SamplingError::InvalidBlobId {
                peer_id: stream.peer_id,
                blob_id,
            }
        })?;
        channel
            .request_sender
            .send(request)
            .map_err(|request| SamplingError::RequestChannel {
                request,
                peer_id: stream.peer_id,
            })?;
        let response: sampling::SampleResponse = channel
            .response_receiver
            .await
            .map_err(|error| SamplingError::ResponseChannel {
                error,
                peer_id: stream.peer_id,
            })?
            .into();
        pack_to_writer(&response, &mut stream.stream)
            .map_err(|error| SamplingError::Io {
                peer_id: stream.peer_id,
                error,
            })
            .await?;
        stream
            .stream
            .flush()
            .await
            .map_err(|error| SamplingError::Io {
                peer_id: stream.peer_id,
                error,
            })?;
        Ok(stream)
    }

    /// Schedule an incoming stream to be replied
    /// Creates the necessary channels so requests can be replied from outside of this behaviour
    /// from whoever that takes the channels
    fn schedule_incoming_stream_task(
        incoming_tasks: &mut FuturesUnordered<IncomingStreamHandlerFuture>,
        sample_stream: SampleStream,
    ) -> (Receiver<BehaviourSampleReq>, Sender<BehaviourSampleRes>) {
        let (request_sender, request_receiver) = oneshot::channel();
        let (response_sender, response_receiver) = oneshot::channel();
        let channel = ResponseChannel {
            request_sender,
            response_receiver,
        };
        incoming_tasks.push(Self::handle_incoming_stream(sample_stream, channel).boxed());
        (request_receiver, response_sender)
    }
}

impl<Membership: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static>
    SamplingBehaviour<Membership>
{
    /// Schedule a new task for sample the blob, if stream is not available queue messages for later
    /// processing.
    #[allow(clippy::too_many_arguments)]
    fn sample(
        peer_id: PeerId,
        outgoing_tasks: &mut FuturesUnordered<OutgoingStreamHandlerFuture>,
        membership: &mut Membership,
        connected_peers: &mut HashSet<PeerId>,
        to_sample: &mut HashMap<PeerId, VecDeque<(Membership::NetworkId, BlobId)>>,
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
        control: &Control,
    ) {
        let members = membership.members_of(&subnetwork_id);
        // TODO: peer selection for sampling should be randomly selected (?) filtering ourselves
        // currently we assume optimal setup which is one peer per blob
        let peer = members
            .iter()
            .filter(|&id| id != &peer_id)
            .copied()
            .next()
            .expect("At least a single node should be a member of the subnetwork");
        // if its connected means we are already working on some other sample, enqueue message
        if connected_peers.contains(&peer) {
            to_sample
                .entry(peer)
                .or_default()
                .push_back((subnetwork_id, blob_id));
        } else {
            connected_peers.insert(peer);
            let control = control.clone();
            let sample_request = sampling::SampleRequest::new(blob_id, subnetwork_id);
            let with_dial_task: OutgoingStreamHandlerFuture = async move {
                let stream = Self::open_stream(peer, control).await?;
                Self::stream_sample(stream, sample_request, subnetwork_id, blob_id).await
            }
            .boxed();
            outgoing_tasks.push(with_dial_task);
        }
    }

    /// Auxiliary method that transforms a sample response into an event
    fn handle_sample_response(
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
        sample_response: sampling::SampleResponse,
        peer_id: PeerId,
    ) -> Option<Poll<ToSwarm<<Self as NetworkBehaviour>::ToSwarm, THandlerInEvent<Self>>>> {
        match sample_response {
            sampling::SampleResponse::Error(error) => Some(Poll::Ready(ToSwarm::GenerateEvent(
                SamplingEvent::SamplingError {
                    error: SamplingError::Protocol {
                        subnetwork_id,
                        error,
                        peer_id,
                    },
                },
            ))),
            sampling::SampleResponse::Blob(blob) => Some(Poll::Ready(ToSwarm::GenerateEvent(
                SamplingEvent::SamplingSuccess {
                    blob_id,
                    subnetwork_id,
                    blob: Box::new(blob.data),
                },
            ))),
        }
    }
}

impl<M: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static> NetworkBehaviour
    for SamplingBehaviour<M>
{
    type ConnectionHandler = Either<
        <libp2p_stream::Behaviour as NetworkBehaviour>::ConnectionHandler,
        libp2p::swarm::dummy::ConnectionHandler,
    >;
    type ToSwarm = SamplingEvent;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.membership.is_allowed(&peer) {
            return Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler));
        }
        self.stream_behaviour
            .handle_established_inbound_connection(connection_id, peer, local_addr, remote_addr)
            .map(Either::Left)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.membership.is_allowed(&peer) {
            return Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler));
        }
        self.stream_behaviour
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
            .map(Either::Left)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.stream_behaviour.on_swarm_event(event)
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
            peer_id,
            outgoing_tasks,
            incoming_tasks,
            to_sample,
            samples_request_stream,
            connected_peers,
            incoming_streams,
            membership,
            control,
            ..
        } = self;
        // poll pending outgoing samples
        if let Poll::Ready(Some((subnetwork_id, blob_id))) =
            samples_request_stream.poll_next_unpin(cx)
        {
            Self::sample(
                *peer_id,
                outgoing_tasks,
                membership,
                connected_peers,
                to_sample,
                subnetwork_id,
                blob_id,
                control,
            );
        }
        // poll outgoing tasks
        if let Poll::Ready(Some(future_result)) = outgoing_tasks.poll_next_unpin(cx) {
            match future_result {
                Ok((blob_id, subnetwork_id, sample_response, stream)) => {
                    let peer_id = stream.peer_id;
                    // handle the free stream then return the success
                    Self::handle_outgoing_stream(
                        outgoing_tasks,
                        to_sample,
                        connected_peers,
                        stream,
                    );
                    // return an error if there was an error on the other side of the wire
                    if let Some(event) = Self::handle_sample_response(
                        blob_id,
                        subnetwork_id,
                        sample_response,
                        peer_id,
                    ) {
                        return event;
                    }
                }
                // Something went up on our side of the wire, bubble it up
                Err(error) => {
                    connected_peers.remove(error.peer_id());
                    return Poll::Ready(ToSwarm::GenerateEvent(SamplingEvent::SamplingError {
                        error,
                    }));
                }
            }
        }
        // poll incoming streams
        if let Poll::Ready(Some((peer_id, stream))) = incoming_streams.poll_next_unpin(cx) {
            let sample_stream = SampleStream { stream, peer_id };
            let (request_receiver, response_sender) =
                Self::schedule_incoming_stream_task(incoming_tasks, sample_stream);
            return Poll::Ready(ToSwarm::GenerateEvent(SamplingEvent::IncomingSample {
                request_receiver,
                response_sender,
            }));
        }
        // poll incoming tasks
        if let Poll::Ready(Some(res)) = incoming_tasks.poll_next_unpin(cx) {
            match res {
                Ok(sample_stream) => {
                    let (request_receiver, response_sender) =
                        Self::schedule_incoming_stream_task(incoming_tasks, sample_stream);
                    return Poll::Ready(ToSwarm::GenerateEvent(SamplingEvent::IncomingSample {
                        request_receiver,
                        response_sender,
                    }));
                }
                Err(error) => {
                    return Poll::Ready(ToSwarm::GenerateEvent(SamplingEvent::SamplingError {
                        error,
                    }))
                }
            }
        }
        // Deal with connection as the underlying behaviour would do
        match self.stream_behaviour.poll(cx) {
            Poll::Ready(ToSwarm::Dial { mut opts }) => {
                // attach known peer address if possible
                if let Some(address) = opts
                    .get_peer_id()
                    .and_then(|peer_id: PeerId| self.addresses.get_address(&peer_id))
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

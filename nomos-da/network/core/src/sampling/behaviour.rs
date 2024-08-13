// std
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::task::{Context, Poll};
// crates
use either::Either;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use futures::stream::{BoxStream, FuturesUnordered};
use futures::{AsyncWriteExt, FutureExt, StreamExt};
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::{Control, IncomingStreams, OpenStreamError};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
// internal
use crate::protocol::{DISPERSAL_PROTOCOL, SAMPLING_PROTOCOL};
use crate::SubnetworkId;
use kzgrs_backend::common::blob::DaBlob;
use nomos_da_messages::common::Blob;
use nomos_da_messages::dispersal::message::MessageType;
use nomos_da_messages::dispersal::DispersalRes;
use nomos_da_messages::sampling::{sample_res, SampleErr, SampleReq, SampleRes};
use nomos_da_messages::{pack_message, unpack_from_reader};
use subnetworks_assignations::MembershipHandler;

#[derive(Debug, Error)]
pub enum SamplingError {
    #[error("Stream disconnected: {error}")]
    Io {
        error: std::io::Error,
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
    },
    #[error("Dispersal response error: {error:?}")]
    Protocol {
        subnetwork_id: SubnetworkId,
        error: SampleErr,
    },
    #[error("Error dialing peer [{peer_id}]: {error}")]
    OpenStream {
        peer_id: PeerId,
        error: OpenStreamError,
    },
    #[error("Unable to deserialize blob response")]
    Deserialize {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
        peer_id: PeerId,
    },
}

type BlobId = [u8; 32];

#[derive(Debug)]
pub enum SamplingEvent {
    /// A blob successfully arrived its destination
    SamplingSuccess {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
        blob: DaBlob,
    },
    IncomingSample {
        request: SampleReq,
        response_sender: oneshot::Sender<SampleRes>,
    },
    SamplingError {
        error: SamplingError,
    },
}

struct SampleStream {
    stream: Stream,
    peer_id: PeerId,
}

type StreamHandlerFutureSuccess = (BlobId, SubnetworkId, SampleRes, SampleStream);
type StreamHandlerFuture = BoxFuture<'static, Result<StreamHandlerFutureSuccess, SamplingError>>;

/// Executor dispersal protocol
/// Do not handle incoming connections, just accepts outgoing ones.
/// It takes care of sending blobs to different subnetworks.
/// Bubbles up events with the success or error when dispersing
pub struct SamplingBehaviour<Membership: MembershipHandler> {
    /// Underlying stream behaviour
    stream_behaviour: libp2p_stream::Behaviour,
    /// Incoming sample request streams
    incoming_streams: IncomingStreams,
    /// Underlying stream control
    control: Control,
    /// Pending running tasks (one task per stream)
    outgoing_tasks: FuturesUnordered<StreamHandlerFuture>,
    /// Subnetworks membership information
    membership: Membership,
    /// Pending blobs that need to be dispersed by PeerId
    to_sample: HashMap<PeerId, VecDeque<(Membership::NetworkId, BlobId)>>,
    /// Already connected peers connection Ids
    connected_peers: HashMap<PeerId, ConnectionId>,
    /// Hook of pending samples channel
    pending_blobs_sender: UnboundedSender<(Membership::NetworkId, BlobId)>,
    /// Pending samples stream
    pending_blobs_stream: BoxStream<'static, (Membership::NetworkId, BlobId)>,
}

impl<Membership> SamplingBehaviour<Membership>
where
    Membership: MembershipHandler + 'static,
    Membership::NetworkId: Send,
{
    pub fn new(membership: Membership) -> Self {
        let stream_behaviour = libp2p_stream::Behaviour::new();
        let mut control = stream_behaviour.new_control();

        let incoming_streams = control
            .accept(SAMPLING_PROTOCOL)
            .expect("Just a single accept to protocol is valid");

        let tasks = FuturesUnordered::new();
        let to_sample = HashMap::new();

        let (pending_blobs_sender, receiver) = mpsc::unbounded_channel();
        let pending_blobs_stream = UnboundedReceiverStream::new(receiver).boxed();
        let connected_peers = HashMap::new();
        Self {
            stream_behaviour,
            incoming_streams,
            control,
            outgoing_tasks: tasks,
            membership,
            to_sample,
            connected_peers,
            pending_blobs_sender,
            pending_blobs_stream,
        }
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
    pub fn samples_sender(&self) -> UnboundedSender<(Membership::NetworkId, BlobId)> {
        self.pending_blobs_sender.clone()
    }

    /// Task for handling streams, one message at a time
    /// Writes the request to the stream and waits for a response
    async fn stream_sample(
        mut stream: SampleStream,
        message: SampleReq,
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
    ) -> Result<StreamHandlerFutureSuccess, SamplingError> {
        stream
            .stream
            .write_all(&pack_message(&message).map_err(|error| SamplingError::Io {
                error,
                blob_id,
                subnetwork_id,
            })?)
            .await
            .map_err(|error| SamplingError::Io {
                error,
                blob_id,
                subnetwork_id,
            })?;
        stream
            .stream
            .flush()
            .await
            .map_err(|error| SamplingError::Io {
                error,
                blob_id,
                subnetwork_id,
            })?;
        let response: SampleRes =
            unpack_from_reader(&mut stream.stream)
                .await
                .map_err(|error| SamplingError::Io {
                    error,
                    blob_id,
                    subnetwork_id,
                })?;
        // Safety: blob_id should always be a 32bytes hash, currently is abstracted into a `Vec<u8>`
        // but probably we should have a `[u8; 32]` wrapped in a custom type `BlobId`
        // TODO: use blob_id when changing types to [u8; 32]
        Ok((blob_id.try_into().unwrap(), subnetwork_id, response, stream))
    }

    /// Get a pending request if its available
    fn next_request(
        peer_id: &PeerId,
        to_sample: &mut HashMap<PeerId, VecDeque<(SubnetworkId, BlobId)>>,
    ) -> Option<(SubnetworkId, BlobId)> {
        to_sample
            .get_mut(peer_id)
            .and_then(|queue| queue.pop_front())
    }

    /// Handler outgoing stream
    /// Schedule a new task if its available or drop the stream if not
    fn handle_outgoing_stream(
        outgoing_tasks: &mut FuturesUnordered<StreamHandlerFuture>,
        to_sample: &mut HashMap<PeerId, VecDeque<(SubnetworkId, BlobId)>>,
        connected_peers: &mut HashMap<PeerId, ConnectionId>,
        stream: SampleStream,
    ) {
        let peer = stream.peer_id;
        // If there is a pending task schedule next one
        if let Some((subnetwork_id, blob_id)) = to_sample.entry(peer).or_default().pop_front() {
            let sample_request = SampleReq {
                blob_id: blob_id.to_vec(),
            };
            outgoing_tasks
                .push(Self::stream_sample(stream, sample_request, subnetwork_id, blob_id).boxed());
        // if not pop stream from connected ones
        } else {
            connected_peers.remove(&peer);
        }
    }
}

impl<Membership: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static>
    SamplingBehaviour<Membership>
{
    /// Schedule a new task for sample the blob, if stream is not available queue messages for later
    /// processing.
    fn sample(
        outgoing_tasks: &mut FuturesUnordered<StreamHandlerFuture>,
        membership: &mut Membership,
        connected_subnetworks: &mut HashMap<PeerId, ConnectionId>,
        to_sample: &mut HashMap<PeerId, VecDeque<(Membership::NetworkId, BlobId)>>,
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
        control: &Control,
    ) {
        let members = membership.members_of(&subnetwork_id);
        let peer = members
            .iter()
            .copied()
            .next()
            .expect("At least a single node should be a member of the subnetwork");
        // if its connected means we are already working on some other sample, enqueue message
        if connected_subnetworks.contains_key(&peer) {
            to_sample
                .entry(peer.clone())
                .or_default()
                .push_back((subnetwork_id, blob_id.clone()));
        } else {
            let mut control = control.clone();
            let sample_request = SampleReq {
                blob_id: blob_id.to_vec(),
            };
            let with_dial_task: StreamHandlerFuture = async move {
                let stream = control
                    .open_stream(peer, DISPERSAL_PROTOCOL)
                    .await
                    .map_err(|error| SamplingError::OpenStream {
                        peer_id: peer,
                        error,
                    })?;
                let stream = SampleStream {
                    stream,
                    peer_id: peer,
                };
                Self::stream_sample(stream, sample_request, subnetwork_id, blob_id).await
            }
            .boxed();
            outgoing_tasks.push(with_dial_task);
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
        self.connected_peers.insert(peer, connection_id);
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
        let Either::Left(event) = event else {
            unreachable!()
        };
        self.stream_behaviour
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        let Self {
            outgoing_tasks,
            to_sample,
            pending_blobs_stream,
            membership,
            connected_peers,
            ..
        } = self;
        // poll pending tasks
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
                    match sample_response {
                        SampleRes {
                            message_type: Some(sample_res::MessageType::Err(error)),
                        } => {
                            return Poll::Ready(ToSwarm::GenerateEvent(
                                SamplingEvent::SamplingError {
                                    error: SamplingError::Protocol {
                                        subnetwork_id,
                                        error,
                                    },
                                },
                            ));
                        }
                        SampleRes {
                            message_type: Some(sample_res::MessageType::Blob(da_blob)),
                        } => {
                            let blob = bincode::deserialize::<DaBlob>(da_blob.data.as_slice())
                                .map_err(|error| SamplingError::Deserialize {
                                    blob_id,
                                    subnetwork_id,
                                    peer_id,
                                });
                            match blob {
                                Ok(blob) => {
                                    return Poll::Ready(ToSwarm::GenerateEvent(
                                        SamplingEvent::SamplingSuccess {
                                            blob_id,
                                            subnetwork_id,
                                            blob,
                                        },
                                    ));
                                }
                                Err(error) => {
                                    return Poll::Ready(ToSwarm::GenerateEvent(
                                        SamplingEvent::SamplingError { error },
                                    ))
                                }
                            }
                        }
                        _ => {
                            unreachable!();
                        }
                    }
                }
                // Something went up on our side of the wire, bubble it up
                Err(error) => {
                    return Poll::Ready(ToSwarm::GenerateEvent(SamplingEvent::SamplingError {
                        error,
                    }))
                }
            }
        }
        // Deal with connection as the underlying behaviour would do
        match self.stream_behaviour.poll(cx) {
            Poll::Ready(ToSwarm::Dial { opts }) => Poll::Ready(ToSwarm::Dial { opts }),
            Poll::Pending => {
                // TODO: probably must be smarter when to wake this
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            _ => unreachable!(),
        }
    }
}

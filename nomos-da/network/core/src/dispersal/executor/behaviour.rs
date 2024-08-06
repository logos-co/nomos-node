// std
use std::collections::{HashMap, VecDeque};
use std::task::{Context, Poll};
// crates
use either::Either;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{AsyncWriteExt, FutureExt, StreamExt};
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::OpenStreamError;
use thiserror::Error;
// internal
use crate::protocol::DISPERSAL_PROTOCOL;
use crate::SubnetworkId;
use kzgrs_backend::common::blob::DaBlob;
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
type BlobId = [u8; 32];
pub enum DispersalExecutorEvent {
    DispersalSuccess {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
    },
    DispersalError {
        error: DispersalError,
    },
}

struct DispersalStream {
    stream: Stream,
    peer_id: PeerId,
}

type StreamHandlerFutureSuccess = (BlobId, SubnetworkId, DispersalRes, DispersalStream);
type StreamHandlerFuture = BoxFuture<'static, Result<StreamHandlerFutureSuccess, DispersalError>>;

pub struct DispersalExecutorBehaviour<Membership> {
    /// Underlying stream behaviour
    stream_behaviour: libp2p_stream::Behaviour,
    /// Pending running tasks (one task per stream)
    tasks: FuturesUnordered<StreamHandlerFuture>,
    /// Streams which didn't have any pending task
    idle_streams: HashMap<PeerId, DispersalStream>,
    /// Subnetworks membership information
    membership: Membership,
    /// Pending blobs that need to be dispersed by PeerId
    to_disperse: HashMap<PeerId, VecDeque<(SubnetworkId, DaBlob)>>,
    /// Already connected peers connection Ids
    connected_subnetworks: HashMap<PeerId, ConnectionId>,
}

impl<Membership: MembershipHandler + 'static> DispersalExecutorBehaviour<Membership> {
    pub fn new(membership: Membership) -> Self {
        let stream_behaviour = libp2p_stream::Behaviour::new();
        let tasks = FuturesUnordered::new();
        let to_disperse = HashMap::new();
        let connected_subnetworks = HashMap::new();
        let idle_streams = HashMap::new();
        Self {
            stream_behaviour,
            tasks,
            membership,
            to_disperse,
            connected_subnetworks,
            idle_streams,
        }
    }

    pub async fn open_stream(&mut self, peer_id: PeerId) -> Result<(), DispersalError> {
        let stream = self
            .stream_behaviour
            .new_control()
            .open_stream(peer_id, DISPERSAL_PROTOCOL)
            .await
            .map_err(|error| DispersalError::OpenStreamError { peer_id, error })?;
        let Self {
            tasks,
            to_disperse,
            idle_streams,
            ..
        } = self;
        Self::handle_stream(
            tasks,
            to_disperse,
            idle_streams,
            DispersalStream { stream, peer_id },
        );
        Ok(())
    }
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
    pub fn disperse_blob(&mut self, subnetwork_id: SubnetworkId, blob: DaBlob) {
        let Self {
            membership,
            connected_subnetworks,
            to_disperse,
            ..
        } = self;
        let members = membership.members_of(&subnetwork_id);
        let peers = members
            .iter()
            .filter(|peer_id| connected_subnetworks.contains_key(peer_id));
        // We may be connected to more than a single node. Usually will be one, but that is an
        // internal decision of the executor itself.
        for peer in peers {
            if let Some(stream) = self.idle_streams.remove(peer) {
                // push a task if the stream is inmediately available
                let fut = Self::stream_disperse(stream, blob.clone(), subnetwork_id).boxed();
                self.tasks.push(fut);
            } else {
                // otherwise queue the blob
                to_disperse
                    .entry(*peer)
                    .or_default()
                    .push_back((subnetwork_id, blob.clone()));
            }
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
            tasks,
            to_disperse,
            idle_streams,
            ..
        } = self;
        // poll pending tasks
        if let Poll::Ready(Some(future_result)) = tasks.poll_next_unpin(cx) {
            match future_result {
                Ok((blob_id, subnetwork_id, dispersal_response, stream)) => {
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
                    // handle the free stream then return the success
                    Self::handle_stream(tasks, to_disperse, idle_streams, stream);
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
        // Deal with connection as the underlying behaviour would do
        match self.stream_behaviour.poll(cx) {
            Poll::Ready(ToSwarm::Dial { opts }) => Poll::Ready(ToSwarm::Dial { opts }),
            Poll::Pending => Poll::Pending,
            _ => unreachable!(),
        }
    }
}

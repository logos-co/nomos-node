use crate::protocol::SAMPLING_PROTOCOL;
use crate::sampling::protocol::{SamplingProtocolRequest, SamplingProtocolResponse};
use crate::SubnetworkId;
use either::Either;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{AsyncWriteExt, FutureExt, StreamExt};
use libp2p::core::upgrade::ReadyUpgrade;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, OneShotHandler, THandler,
    THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream, StreamProtocol};
use libp2p_stream::{Control, IncomingStreams};
use log::error;
use nomos_da_messages::sampling::{SampleReq, SampleRes};
use nomos_da_messages::{pack_message, unpack_from_reader};
use std::collections::HashMap;
use std::future::Future;
use std::task::{Context, Poll};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio_stream::wrappers::UnboundedReceiverStream;

type BlobId = [u8; 32];

#[derive(Debug, thiserror::Error)]
pub enum SampleError {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("A response couldn't be received: {0}")]
    ResponseError(#[from] oneshot::error::RecvError),
    #[error("Unable to handle sample request: {0:?}")]
    UnhandledRequest(SampleReq),
    #[error(transparent)]
    OpenStream(#[from] libp2p_stream::OpenStreamError),
}

#[derive(Debug)]
pub enum SamplingEvent {
    IncomingSample {
        request_receiver: oneshot::Receiver<SampleReq>,
        response_sender: oneshot::Sender<SampleRes>,
    },
    SampleResponse(SampleRes),
    SampleError(SampleError),
}

#[derive(Debug)]
enum SamplingProtocolEvent {
    IncomingSample(SamplingProtocolResponse),
    SampleResponse(SampleRes),
}

impl From<SamplingProtocolRequest> for SamplingProtocolEvent {
    fn from(SamplingProtocolRequest(value): SamplingProtocolRequest) -> Self {
        Self::IncomingSample(value)
    }
}

impl From<SamplingProtocolResponse> for SamplingProtocolEvent {
    fn from(value: SamplingProtocolResponse) -> Self {
        Self::IncomingSample(value)
    }
}

impl From<()> for SamplingProtocolEvent {
    fn from(value: ()) -> Self {
        unreachable!()
    }
}

pub struct SamplingBehaviour<Membership: MembershipHandler> {
    peer_id: PeerId,
    /// Already connected peers connection Ids
    connected_subnetworks: HashMap<PeerId, ConnectionId>,
    outgoing_sender: UnboundedSender<(Membership::NetworkId, BlobId)>,
    outgoing_samples: UnboundedReceiverStream<(Membership::NetworkId, BlobId)>,
    membership: Membership,
    outgoing_tasks: FuturesUnordered<BoxFuture<'static, Result<SampleRes, SampleError>>>,
    incoming_tasks: FuturesUnordered<BoxFuture<'static, Result<(), SampleError>>>,
}

impl<Membership: MembershipHandler> SamplingBehaviour<Membership> {
    pub fn new(peer_id: PeerId, membership: Membership) -> Self {
        let connected_subnetworks = HashMap::new();
        let (outgoing_sender, outgoing_receiver) =
            tokio::sync::mpsc::unbounded_channel::<(Membership::NetworkId, BlobId)>();
        let outgoing_samples = UnboundedReceiverStream::new(outgoing_receiver);
        Self {
            peer_id,
            connected_subnetworks,
            membership,
            outgoing_sender,
            outgoing_samples,
            outgoing_tasks: Default::default(),
            incoming_tasks: Default::default(),
        }
    }

    pub fn sample_request_channel(&self) -> UnboundedSender<(Membership::NetworkId, BlobId)> {
        self.outgoing_sender.clone()
    }

    fn handle_incoming_stream(
        mut stream: Stream,
    ) -> (
        oneshot::Receiver<SampleReq>,
        oneshot::Sender<SampleRes>,
        impl Future<Output = Result<(), SampleError>>,
    ) {
        let (request_sender, request_receiver) = oneshot::channel();
        let (response_sender, response_receiver) = oneshot::channel();
        let fut = async move {
            let msg: SampleReq = unpack_from_reader(&mut stream)
                .await
                .map_err(SampleError::IO)?;
            request_sender
                .send(msg)
                .map_err(SampleError::UnhandledRequest)?;
            // TODO: maybe timeout here? If we are too busy this may be dropped
            let response: SampleRes = response_receiver
                .await
                .map_err(SampleError::ResponseError)?;
            stream
                .write_all(&pack_message(&response).map_err(SampleError::IO)?)
                .await
                .map_err(SampleError::IO)?;
            stream.flush().await.map_err(SampleError::IO)?;
            Ok(())
        };
        (request_receiver, response_sender, fut)
    }
}

impl<Membership> SamplingBehaviour<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    fn handle_sample_request(
        peer_id: &PeerId,
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
        membership: &Membership,
        control: &mut Control,
        stream: Option<Stream>,
    ) -> impl Future<Output = Result<(SampleRes, Stream), SampleError>> {
        // TODO: somehow we need to hook either peer selection (which can come from outside).
        let Some(peer) = membership
            .members_of(&subnetwork_id)
            .iter()
            .filter(|&id| id != peer_id) // filter out ourselves
            .copied()
            .next()
        else {
            unreachable!();
        };
        // TODO: check on possible backpressure issues
        let mut control = control.clone();
        async move {
            let mut stream = if let Some(stream) = stream {
                stream
            } else {
                control.open_stream(peer, SAMPLING_PROTOCOL).await?
            };
            let request = SampleReq {
                blob_id: blob_id.to_vec(),
            };
            stream
                .write_all(&pack_message(&request).map_err(SampleError::IO)?)
                .await
                .map_err(SampleError::IO)?;
            stream.flush().await.map_err(SampleError::IO)?;
            let response: SampleRes = unpack_from_reader(&mut stream)
                .await
                .map_err(SampleError::IO)?;
            Ok((response, stream))
        }
    }
}

impl<Membership> NetworkBehaviour for SamplingBehaviour<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
{
    type ConnectionHandler = Either<
        OneShotHandler<SamplingProtocolRequest, SamplingProtocolResponse, SamplingProtocolEvent>,
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
        self.connected_subnetworks.insert(peer, connection_id);
        Ok(Either::Left(OneShotHandler::default()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Either::Left(OneShotHandler::default()))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let Either::Left(event) = event else {
            unreachable!()
        };
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        //     let Self {
        //         peer_id,
        //         incoming_streams,
        //         outgoing_samples,
        //         membership,
        //         outgoing_tasks,
        //         incoming_tasks,
        //         ..
        //     } = self;
        //     // poll incoming streams
        //     // dispersal streams are considered one shot, they are dropped after completion
        //     if let Poll::Ready(Some((_peer, stream))) = incoming_streams.poll_next_unpin(cx) {
        //         let (request_receiver, response_sender, fut) = Self::handle_incoming_stream(stream);
        //         let event = ToSwarm::GenerateEvent(SamplingEvent::IncomingSample {
        //             request_receiver,
        //             response_sender,
        //         });
        //         incoming_tasks.push(fut.boxed());
        //         return Poll::Ready(event);
        //     }
        //     // poll outgoing streams
        //     if let Poll::Ready(Some((subnetwork_id, blob_id))) = outgoing_samples.poll_next_unpin(cx) {
        //         let fut = Self::handle_sample_request(peer_id, subnetwork_id, blob_id, membership);
        //         outgoing_tasks.push(fut.boxed());
        //     }
        //     if let Poll::Ready(Some(Err(e))) = incoming_tasks.poll_next_unpin(cx) {
        //         error!("Sample response error: {e}");
        //     }
        //     if let Poll::Ready(Some(res)) = outgoing_tasks.poll_next_unpin(cx) {
        //         let event = match res {
        //             Ok(response) => SamplingEvent::SampleResponse(response),
        //             Err(error) => SamplingEvent::SampleError(error),
        //         };
        //         return Poll::Ready(ToSwarm::GenerateEvent(event));
        //     }
        //
        //     cx.waker().wake_by_ref();
        //     Poll::Pending
        Poll::Pending
    }
}

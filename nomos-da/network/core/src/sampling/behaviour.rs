use crate::protocol::SAMPLING_PROTOCOL;
use crate::SubnetworkId;
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

pub enum SamplingEvent {
    IncomingSample {
        request_receiver: oneshot::Receiver<SampleReq>,
        response_sender: oneshot::Sender<SampleRes>,
    },
    SampleResponse(SampleRes),
    SampleError(SampleError),
}

pub struct SamplingBehaviour<Membership: MembershipHandler> {
    behaviour: libp2p_stream::Behaviour,
    stream_control: libp2p_stream::Control,
    /// Already connected peers connection Ids
    connected_subnetworks: HashMap<PeerId, ConnectionId>,
    incoming_streams: IncomingStreams,
    outgoing_sender: UnboundedSender<(Membership::NetworkId, BlobId)>,
    outgoing_samples: UnboundedReceiverStream<(Membership::NetworkId, BlobId)>,
    membership: Membership,
    outgoing_tasks: FuturesUnordered<BoxFuture<'static, Result<SampleRes, SampleError>>>,
    incoming_tasks: FuturesUnordered<BoxFuture<'static, Result<(), SampleError>>>,
}

impl<Membership: MembershipHandler> SamplingBehaviour<Membership> {
    pub fn new(membership: Membership) -> Self {
        let behaviour = libp2p_stream::Behaviour::new();
        let mut stream_control = behaviour.new_control();
        let incoming_streams = stream_control
            .accept(SAMPLING_PROTOCOL)
            .expect("Just a single accept to protocol is valid");
        let connected_subnetworks = HashMap::new();
        let (outgoing_sender, outgoing_receiver) =
            tokio::sync::mpsc::unbounded_channel::<(Membership::NetworkId, BlobId)>();
        let outgoing_samples = UnboundedReceiverStream::new(outgoing_receiver);
        Self {
            behaviour,
            stream_control,
            connected_subnetworks,
            incoming_streams,
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
        subnetwork_id: SubnetworkId,
        blob_id: BlobId,
        membership: &Membership,
        control: &mut Control,
    ) -> impl Future<Output = Result<SampleRes, SampleError>> {
        // TODO: somehow we need to hook either peer selection (which can come from outside).
        let Some(peer) = membership.members_of(&subnetwork_id).iter().copied().next() else {
            unreachable!();
        };
        // TODO: check on possible backpressure issues
        let mut control = control.clone();
        async move {
            let mut stream = control.open_stream(peer, SAMPLING_PROTOCOL).await?;
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
            Ok(response)
        }
    }
}

impl<Membership> NetworkBehaviour for SamplingBehaviour<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
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
        self.connected_subnetworks.insert(peer, connection_id);
        self.behaviour
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
        self.behaviour
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
            .map(Either::Left)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.behaviour.on_swarm_event(event)
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
        self.behaviour
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        let Self {
            stream_control,
            incoming_streams,
            outgoing_samples,
            membership,
            outgoing_tasks,
            incoming_tasks,
            ..
        } = self;
        // poll incoming streams
        // dispersal streams are considered one shot, they are dropped after completion
        if let Poll::Ready(Some((_peer, stream))) = incoming_streams.poll_next_unpin(cx) {
            let (request_receiver, response_sender, fut) = Self::handle_incoming_stream(stream);
            let event = ToSwarm::GenerateEvent(SamplingEvent::IncomingSample {
                request_receiver,
                response_sender,
            });
            incoming_tasks.push(fut.boxed());
            return Poll::Ready(event);
        }
        // poll outgoing streams
        if let Poll::Ready(Some((subnetwork_id, blob_id))) = outgoing_samples.poll_next_unpin(cx) {
            let fut =
                Self::handle_sample_request(subnetwork_id, blob_id, membership, stream_control);
            outgoing_tasks.push(fut.boxed());
        }
        if let Poll::Ready(Some(Err(e))) = incoming_tasks.poll_next_unpin(cx) {
            error!("Sample response error: {e}");
        }
        if let Poll::Ready(Some(res)) = outgoing_tasks.poll_next_unpin(cx) {
            let event = match res {
                Ok(response) => SamplingEvent::SampleResponse(response),
                Err(error) => SamplingEvent::SampleError(error),
            };
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }
        // TODO: handle this smarter
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

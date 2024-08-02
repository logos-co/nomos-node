use crate::protocol::DISPERSAL_PROTOCOL;
use crate::SubnetworkId;
use either::Either;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt};
use kzgrs_backend::common::blob::DaBlob;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::IncomingStreams;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::Error;
use std::task::{Context, Poll};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

type BlobId = [u8; 32];
pub enum DispersalExecutorEvent {
    DispersalSuccess {
        blob_id: Vec<u8>,
        subnetwork_id: SubnetworkId,
    },
}

struct DispersalStream {
    stream: Stream,
    peer_id: PeerId,
}

pub struct DispersalExecutorBehaviour<Membership> {
    stream_behaviour: libp2p_stream::Behaviour,
    incoming_streams: IncomingStreams,
    tasks: FuturesUnordered<BoxFuture<'static, Result<(BlobId, DispersalStream), Error>>>,
    membership: Membership,
    to_disperse: HashMap<PeerId, VecDeque<DaBlob>>,
    connected_subnetworks: HashMap<PeerId, ConnectionId>,
    success_events_receiver: UnboundedReceiverStream<(BlobId, SubnetworkId)>,
    success_events_sender: mpsc::UnboundedSender<(BlobId, SubnetworkId)>,
}

impl<Membership: MembershipHandler> DispersalExecutorBehaviour<Membership> {
    pub fn new(membership: Membership) -> Self {
        let stream_behaviour = libp2p_stream::Behaviour::new();
        let mut stream_control = stream_behaviour.new_control();
        let incoming_streams = stream_control
            .accept(DISPERSAL_PROTOCOL)
            .expect("Just a single accept to protocol is valid");
        let tasks = FuturesUnordered::new();
        let to_disperse = HashMap::new();
        let connected_subnetworks = HashMap::new();
        let (success_events_sender, success_events_receiver) =
            mpsc::unbounded_channel::<(BlobId, SubnetworkId)>();
        let success_events_receiver = UnboundedReceiverStream::from(success_events_receiver);
        Self {
            stream_behaviour,
            incoming_streams,
            tasks,
            membership,
            to_disperse,
            connected_subnetworks,
            success_events_sender,
            success_events_receiver,
        }
    }
    fn handle_dispersal_stream(
        stream: DispersalStream,
    ) -> impl Future<Output = Result<(BlobId, DispersalStream), Error>> {
        async { Ok(([0; 32], stream)) }
    }
}

impl<Membership: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId>>
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
        for peer in peers {
            to_disperse
                .entry(*peer)
                .or_default()
                .push_back(blob.clone());
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
            incoming_streams,
            tasks,
            membership,
            success_events_receiver,
            ..
        } = self;
        if let Poll::Ready(Some((blob_id, subnetwork_id))) =
            success_events_receiver.next().poll_unpin(cx)
        {
            return Poll::Ready(ToSwarm::GenerateEvent(
                DispersalExecutorEvent::DispersalSuccess {
                    blob_id: blob_id.to_vec(),
                    subnetwork_id,
                },
            ));
        }
        if let Poll::Ready(Some((peer_id, stream))) = incoming_streams.poll_next_unpin(cx) {
            let stream = DispersalStream { stream, peer_id };
            let fut = Self::handle_dispersal_stream(stream).boxed();
            tasks.push(fut);
        }
        // Deal with connection as the underlying behaviour would do
        match self.stream_behaviour.poll(cx) {
            Poll::Ready(ToSwarm::Dial { opts }) => Poll::Ready(ToSwarm::Dial { opts }),
            Poll::Pending => Poll::Pending,
            _ => unreachable!(),
        }
    }
}

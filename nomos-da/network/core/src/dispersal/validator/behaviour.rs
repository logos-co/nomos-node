use std::io::Error;
use crate::protocol::PROTOCOL_NAME;
use crate::replication::handler::DaMessage;
use either::Either;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::IncomingStreams;
use std::task::{Context, Poll};
use subnetworks_assignations::MembershipHandler;

pub enum DispersalEvent {}
pub struct DispersalValidatorBehaviour<Membership> {
    stream_behaviour: libp2p_stream::Behaviour,
    stream_control: libp2p_stream::Control,
    incoming_streams: IncomingStreams,
    tasks: FuturesUnordered<DaMessage>,
    membership: Membership,
}

impl<Membership: MembershipHandler> DispersalValidatorBehaviour<Membership> {
    pub fn new(membership: Membership) -> Self {
        let mut stream_behaviour = libp2p_stream::Behaviour::new();
        let mut stream_control = stream_behaviour.new_control();
        let incoming_streams = stream_control
            .accept(PROTOCOL_NAME)
            .expect("Just a single accept to protocol is valid");
        let tasks = FuturesUnordered::new();
        Self {
            stream_behaviour,
            stream_control,
            incoming_streams,
            tasks,
            membership,
        }
    }

    async fn handle_new_stream(stream: Stream) -> Result<(DaMessage, Stream), Error> {
        stream.
    }

    async fn on_poll(&mut self) {
        let Self {
            incoming_streams,
            tasks,
            ..
        } = &mut self;
        tokio::select! {
            new_stream = incoming_streams => {
                tasks.push(self.handle_stream(new_stream));
            }
            stream_event =
        }
    }
}

impl<M: MembershipHandler> NetworkBehaviour for DispersalValidatorBehaviour<M> {
    type ConnectionHandler = Either<
        <libp2p_stream::Behaviour as NetworkBehaviour>::ConnectionHandler,
        libp2p::swarm::dummy::ConnectionHandler,
    >;
    type ToSwarm = DispersalEvent;

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
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        unreachable!()
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
        unimplemented!()
    }
}

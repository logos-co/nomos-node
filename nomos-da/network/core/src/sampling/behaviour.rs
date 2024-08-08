use libp2p::core::Endpoint;
use libp2p::swarm::handler::{FullyNegotiatedInbound, FullyNegotiatedOutbound};
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream, StreamProtocol};
use std::task::{Context, Poll};

pub enum SamplingEvent {}

enum HandlerEvent {}
pub struct SamplingBehaviour {}

impl NetworkBehaviour for SamplingBehaviour {
    type ConnectionHandler = libp2p::swarm::handler::OneShotHandler<
        FullyNegotiatedInbound<StreamProtocol, ()>,
        FullyNegotiatedOutbound<StreamProtocol, ()>,
        HandlerEvent,
    >;
    type ToSwarm = ();

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        todo!()
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        todo!()
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        todo!()
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
        todo!()
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        todo!()
    }
}

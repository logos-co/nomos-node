use std::collections::VecDeque;
use std::task::{Context, Poll};

use libp2p::{Multiaddr, PeerId};
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};

use subnetworks_assignations::MembershipHandler;

use super::handler::{BroadcastHandler, DaMessage, HandlerEventToBehaviour};

pub type SubnetworkId = u16;

pub enum BroadcastEvent {
    IncomingMessage { peer_id: PeerId, message: DaMessage },
}

/// Nomos DA broadcas network behaviour
/// This item handles the logic of the nomos da subnetworks broadcasting
/// DA subnetworks are a logical distribution of subsets.
/// A node just connects and accepts connections to other nodes that are in the same subsets.
/// A node forwards messages to all connected peers which are member of the addressed `SubnetworkId`.
pub struct BroadcastBehaviour<Membership> {
    local_peer_id: PeerId,
    membership: Membership,
    handler_events: VecDeque<BroadcastEvent>,
}

impl<M> BroadcastBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    fn is_neighbour(&self, peer_id: &PeerId) -> bool {
        self.membership
            .membership(&self.local_peer_id)
            .intersection(&self.membership.membership(&peer_id))
            .count()
            > 0
    }
}

impl<M> NetworkBehaviour for BroadcastBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
{
    type ConnectionHandler = BroadcastHandler;
    type ToSwarm = BroadcastEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        peer_id: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.is_neighbour(&peer_id) {
            // TODO: use proper error types
            return Err(ConnectionDenied::new(
                "Peer is not a member of our subnetwork",
            ));
        }
        Ok(BroadcastHandler::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(BroadcastHandler::new())
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {
        todo!()
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            HandlerEventToBehaviour::IncomingMessage { message } => {
                self.handler_events
                    .push_back(BroadcastEvent::IncomingMessage { message, peer_id });
            }
            HandlerEventToBehaviour::OutgoingMessageError { .. } => {
                todo!("Retry or report?")
            }
        }
    }

    fn poll(
        &mut self,
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.handler_events.pop_back() {
            Poll::Ready(ToSwarm::GenerateEvent(event))
        } else {
            Poll::Pending
        }
    }
}

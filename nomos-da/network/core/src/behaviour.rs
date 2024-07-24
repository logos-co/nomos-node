use std::collections::{BTreeSet, VecDeque};
use std::task::{Context, Poll};

use libp2p::{Multiaddr, PeerId};
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};

use subnetworks_assignations::MembershipHandler;

use crate::handler::{DaMessage, DaNetworkHandler, HandlerEventToBehaviour};

pub type SubnetworksSize = u16;
pub type SubnetworkId = u16;

pub enum NomosDaEvent {
    IncomingMessage { peer_id: PeerId, message: DaMessage },
}
pub struct NomosDaNetworkBehaviour<Membership> {
    local_peer_id: PeerId,
    nodes: BTreeSet<PeerId>,
    subnetworks_size: SubnetworksSize,
    subscribed_subnetworks: BTreeSet<SubnetworkId>,
    membership: Membership,
    handler_events: VecDeque<NomosDaEvent>,
}

impl<M> NomosDaNetworkBehaviour<M>
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

impl<M> NetworkBehaviour for NomosDaNetworkBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
{
    type ConnectionHandler = DaNetworkHandler;
    type ToSwarm = NomosDaEvent;

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
        Ok(DaNetworkHandler::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(DaNetworkHandler::new())
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
                    .push_back(NomosDaEvent::IncomingMessage { message, peer_id });
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

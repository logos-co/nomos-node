// std
use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll, Waker};

// crates
use either::Either;
use indexmap::IndexSet;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, NotifyHandler, THandler,
    THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use log::{error, trace};

use subnetworks_assignations::MembershipHandler;

use crate::SubnetworkId;

// internal
use super::handler::{
    BehaviourEventToHandler, DaMessage, HandlerEventToBehaviour, ReplicationHandler,
};

type SwarmEvent = ToSwarm<ReplicationEvent, Either<BehaviourEventToHandler, void::Void>>;

/// Nomos DA BroadcastEvents to be bubble up to logic layers
#[allow(dead_code)] // todo: remove when used in tests
#[derive(Debug)]
pub enum ReplicationEvent {
    IncomingMessage { peer_id: PeerId, message: DaMessage },
}

/// Nomos DA broadcas network behaviour
/// This item handles the logic of the nomos da subnetworks broadcasting
/// DA subnetworks are a logical distribution of subsets.
/// A node just connects and accepts connections to other nodes that are in the same subsets.
/// A node forwards messages to all connected peers which are member of the addressed `SubnetworkId`.
pub struct ReplicationBehaviour<Membership> {
    /// Local peer Id, related to the libp2p public key
    local_peer_id: PeerId,
    /// Membership handler, membership handles the subsets logics on who is where in the
    /// nomos DA subnetworks
    membership: Membership,
    /// Relation of connected peers of replication subnetworks
    connected: HashMap<PeerId, ConnectionId>,
    /// Outgoing event queue
    outgoing_events: VecDeque<SwarmEvent>,
    /// Seen messages cache holds a record of seen messages, messages will be removed from this
    /// set after some time to keep it
    seen_message_cache: IndexSet<(Vec<u8>, SubnetworkId)>,
    /// Waker that handles polling
    waker: Option<Waker>,
}

impl<Membership> ReplicationBehaviour<Membership> {
    pub fn new(peer_id: PeerId, membership: Membership) -> Self {
        Self {
            local_peer_id: peer_id,
            membership,
            connected: Default::default(),
            outgoing_events: Default::default(),
            seen_message_cache: Default::default(),
            waker: None,
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        self.membership = membership;
    }
}

impl<M> ReplicationBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    /// Check if some peer membership lies in at least a single subnetwork that the local peer is a
    /// member too.
    fn is_neighbour(&self, peer_id: &PeerId) -> bool {
        self.membership
            .membership(&self.local_peer_id)
            .intersection(&self.membership.membership(peer_id))
            .count()
            > 0
    }

    fn no_loopback_member_peers_of(&self, subnetwork: &SubnetworkId) -> HashSet<PeerId> {
        let mut peers = self.membership.members_of(subnetwork);
        // no loopback
        peers.remove(&self.local_peer_id);
        peers
    }

    fn replicate_message(&mut self, message: DaMessage) {
        let message_id = (
            message.blob.as_ref().unwrap().blob_id.clone(),
            message
                .subnetwork_id
                .try_into()
                .expect("Subnetwork ID should be u16"),
        );
        if self.seen_message_cache.contains(&message_id) {
            return;
        }
        self.seen_message_cache.insert(message_id);
        self.send_message(message)
    }

    pub fn send_message(&mut self, message: DaMessage) {
        // push a message in the queue for every single peer connected that is a member of the
        // selected subnetwork_id
        let peers = self.no_loopback_member_peers_of(
            &message
                .subnetwork_id
                .try_into()
                .expect("Subnetwork ID should be u16"),
        );

        let connected_peers: Vec<_> = self
            .connected
            .iter()
            .filter(|(peer_id, _connection_id)| peers.contains(peer_id))
            .collect();

        for (peer_id, connection_id) in connected_peers {
            self.outgoing_events.push_back(SwarmEvent::NotifyHandler {
                peer_id: *peer_id,
                handler: NotifyHandler::One(*connection_id),
                event: Either::Left(BehaviourEventToHandler::OutgoingMessage {
                    message: message.clone(),
                }),
            })
        }
        self.try_wake();
    }

    pub fn try_wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<M> NetworkBehaviour for ReplicationBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
{
    type ConnectionHandler = Either<ReplicationHandler, libp2p::swarm::dummy::ConnectionHandler>;
    type ToSwarm = ReplicationEvent;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.is_neighbour(&peer_id) {
            trace!("refusing connection to {peer_id}");
            return Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler));
        }
        trace!("{}, Connected to {peer_id}", self.local_peer_id);
        self.connected.insert(peer_id, connection_id);
        Ok(Either::Left(ReplicationHandler::new()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        trace!("{}, Connected to {peer_id}", self.local_peer_id);
        self.connected.insert(peer_id, connection_id);
        Ok(Either::Left(ReplicationHandler::new()))
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let event = match event {
            Either::Left(e) => e,
            Either::Right(v) => void::unreachable(v),
        };
        match event {
            HandlerEventToBehaviour::IncomingMessage { message } => {
                self.replicate_message(message.clone());
                self.outgoing_events.push_back(ToSwarm::GenerateEvent(
                    ReplicationEvent::IncomingMessage { message, peer_id },
                ));
            }
            HandlerEventToBehaviour::OutgoingMessageError { error } => {
                error!("Couldn't send message due to {error}");
                todo!("Retry?")
            }
        }
        self.try_wake();
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.outgoing_events.pop_front() {
            Poll::Ready(event)
        } else {
            self.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

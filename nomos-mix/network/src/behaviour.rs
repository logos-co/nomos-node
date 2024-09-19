use std::{
    collections::{HashMap, HashSet, VecDeque},
    task::{Context, Poll},
};

use cached::{Cached, TimedCache};
use either::Either;
use libp2p::{
    core::Endpoint,
    gossipsub,
    swarm::{
        ConnectionClosed, ConnectionDenied, ConnectionHandler, ConnectionHandlerSelect,
        ConnectionId, FromSwarm, NetworkBehaviour, NotifyHandler, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use nomos_mix_message::{message_id, unwrap_message};

use crate::{
    error::Error,
    handler::{FromBehaviour, MixConnectionHandler, ToBehaviour},
};

const FULLY_UNWRAPPED_MSG_TOPIC: &str = "fully-unwrapped-msg";

pub struct Behaviour {
    mix: MixBehaviour,
    gossipsub: gossipsub::Behaviour,
    topic: gossipsub::IdentTopic,
    pending_events: VecDeque<ToSwarm<Event, THandlerInEvent<Self>>>,
}

#[derive(Debug)]
pub enum Event {
    Mix(MixBehaviourEvent),
    Gossipsub(gossipsub::Event),
    Error(Error),
}

impl From<MixBehaviourEvent> for Event {
    fn from(event: MixBehaviourEvent) -> Self {
        Self::Mix(event)
    }
}

impl From<gossipsub::Event> for Event {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
    }
}

impl Behaviour {
    pub fn new(mix: MixBehaviour, mut gossipsub: gossipsub::Behaviour) -> Result<Self, Error> {
        let topic = gossipsub::IdentTopic::new(FULLY_UNWRAPPED_MSG_TOPIC);
        gossipsub
            .subscribe(&topic)
            .map_err(Error::GossipsubSubscriptionError)?;
        Ok(Self {
            mix,
            gossipsub,
            topic,
            pending_events: VecDeque::new(),
        })
    }

    pub fn publish(&mut self, message: Vec<u8>) -> Result<(), Error> {
        self.mix.publish(message)
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = ConnectionHandlerSelect<
        <MixBehaviour as NetworkBehaviour>::ConnectionHandler,
        <gossipsub::Behaviour as NetworkBehaviour>::ConnectionHandler,
    >;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.mix
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)?;
        self.gossipsub
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mix_handler = self.mix.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        let gossipsub_handler = self.gossipsub.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        Ok(mix_handler.select(gossipsub_handler))
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let mut combined_addresses = Vec::new();
        combined_addresses.extend(self.mix.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?);
        combined_addresses.extend(self.gossipsub.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?);
        Ok(combined_addresses)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mix_handler = self.mix.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )?;
        let gossipsub_handler = self.gossipsub.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )?;
        Ok(mix_handler.select(gossipsub_handler))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.mix.on_swarm_event(event);
        self.gossipsub.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            Either::Left(event) => {
                self.mix
                    .on_connection_handler_event(peer_id, connection_id, event)
            }
            Either::Right(event) => {
                self.gossipsub
                    .on_connection_handler_event(peer_id, connection_id, event)
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // Handle pending events first
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }

        // Handle mix events
        if let Poll::Ready(event) = self.mix.poll(cx) {
            match &event {
                ToSwarm::GenerateEvent(MixBehaviourEvent::FullyUnwrappedMessage(msg)) => {
                    if let Err(e) = self.gossipsub.publish(self.topic.clone(), msg.clone()) {
                        self.pending_events.push_back(
                            ToSwarm::GenerateEvent(Event::Error(Error::GossipsubPublishError(e)))
                                .map_in(Either::Left),
                        );
                    }
                    // Return the fully unwrapped message to the app
                    // because the gossipsub publisher doesn't receive the message.
                    return Poll::Ready(event.map_out(|e| e.into()).map_in(Either::Left));
                }
                _ => {
                    return Poll::Ready(event.map_out(|e| e.into()).map_in(Either::Left));
                }
            }
        }

        // Handle gossipsub events
        if let Poll::Ready(event) = self.gossipsub.poll(cx) {
            return Poll::Ready(event.map_out(|e| e.into()).map_in(Either::Right));
        }

        Poll::Pending
    }
}

/// Network behaviour that handles the mixgossip protocol.
pub struct MixBehaviour {
    /// Connected peers and their connection IDs
    connected_peers: HashMap<PeerId, HashSet<ConnectionId>>,
    /// Queue of events to yield to the swarm.
    events: VecDeque<ToSwarm<MixBehaviourEvent, FromBehaviour>>,
    /// An LRU Time cache for storing seen messages (based on their ID). This cache prevents
    /// duplicates from being propagated to the application and on the network.
    duplicate_cache: TimedCache<Vec<u8>, ()>,
}

#[derive(Debug)]
pub enum MixBehaviourEvent {
    /// A fully unwrapped message received from one of the peers.
    FullyUnwrappedMessage(Vec<u8>),
    Error(Error),
}

impl MixBehaviour {
    pub fn new() -> Self {
        Self {
            connected_peers: HashMap::new(),
            events: VecDeque::new(),
            duplicate_cache: TimedCache::with_lifespan(60),
        }
    }

    pub fn publish(&mut self, message: Vec<u8>) -> Result<(), Error> {
        self.duplicate_cache.cache_set(message_id(&message), ());
        self.forward_message(message, None)
    }

    fn forward_message(
        &mut self,
        message: Vec<u8>,
        propagation_source: Option<PeerId>,
    ) -> Result<(), Error> {
        let peer_ids = self
            .connected_peers
            .keys()
            .filter(|&peer_id| {
                if let Some(propagation_source) = propagation_source {
                    *peer_id != propagation_source
                } else {
                    true
                }
            })
            .cloned()
            .collect::<Vec<_>>();

        if peer_ids.is_empty() {
            return Err(Error::NoPeers);
        }

        for peer_id in peer_ids.into_iter() {
            tracing::debug!("Registering event for peer {:?} to send msg", peer_id);
            self.events.push_back(ToSwarm::NotifyHandler {
                peer_id,
                handler: NotifyHandler::Any,
                event: FromBehaviour::Message(message.clone()),
            });
        }

        Ok(())
    }

    fn add_peer_conn(&mut self, peer_id: PeerId, connection_id: ConnectionId) -> bool {
        tracing::debug!(
            "Adding to connected_peers: peer_id:{:?}, connection_id:{:?}",
            peer_id,
            connection_id
        );
        self.connected_peers
            .entry(peer_id)
            .or_default()
            .insert(connection_id)
    }

    fn remove_peer_conn(&mut self, peer_id: &PeerId, connection_id: &ConnectionId) {
        if let Some(connections) = self.connected_peers.get_mut(peer_id) {
            tracing::debug!(
                "Removing from connected_peers: peer:{:?}, connection_id:{:?}",
                peer_id,
                connection_id
            );
            connections.remove(connection_id);
            if connections.is_empty() {
                self.connected_peers.remove(peer_id);
            }
        }
    }
}

impl NetworkBehaviour for MixBehaviour {
    type ConnectionHandler = MixConnectionHandler;
    type ToSwarm = MixBehaviourEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(MixConnectionHandler::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(MixConnectionHandler::new())
    }

    /// Informs the behaviour about an event from the [`Swarm`].
    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            connection_id,
            ..
        }) = event
        {
            self.remove_peer_conn(&peer_id, &connection_id);
        }
    }

    /// Informs the behaviour about an event generated by the [`ConnectionHandler`]
    /// dedicated to the peer identified by `peer_id`.
    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            ToBehaviour::Message(message) => {
                if self
                    .duplicate_cache
                    .cache_set(message_id(&message), ())
                    .is_some()
                {
                    return;
                }

                match unwrap_message(&message) {
                    Ok((unwrapped_msg, fully_unwrapped)) => {
                        if fully_unwrapped {
                            self.events.push_back(ToSwarm::GenerateEvent(
                                MixBehaviourEvent::FullyUnwrappedMessage(unwrapped_msg),
                            ));
                        } else if let Err(e) = self.forward_message(unwrapped_msg, Some(peer_id)) {
                            tracing::error!("Failed to forward message: {:?}", e);
                        }
                    }
                    Err(nomos_mix_message::Error::MsgUnwrapNotAllowed) => {
                        if let Err(e) = self.forward_message(message, Some(peer_id)) {
                            tracing::error!("Failed to forward message: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to unwrap message: {:?}", e);
                    }
                }
            }
            ToBehaviour::FullyNegotiatedOutbound => {
                self.add_peer_conn(peer_id, connection_id);
            }
            ToBehaviour::IOError(error) => {
                // TODO: Consider removing the peer from the connected_peers and closing the connection
                self.events
                    .push_back(ToSwarm::GenerateEvent(MixBehaviourEvent::Error(
                        Error::PeerIOError {
                            error,
                            peer_id,
                            connection_id,
                        },
                    )));
            }
            ToBehaviour::NegotiationFailed => {
                self.remove_peer_conn(&peer_id, &connection_id);
            }
        }
    }

    /// Polls for things that swarm should do.
    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            Poll::Pending
        }
    }
}

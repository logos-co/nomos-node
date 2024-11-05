use crate::{
    error::Error,
    handler::{FromBehaviour, MixConnectionHandler, ToBehaviour},
};
use cached::{Cached, TimedCache};
use libp2p::{
    core::Endpoint,
    swarm::{
        ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        NotifyHandler, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use nomos_mix_message::MixMessage;
use sha2::{Digest, Sha256};
use std::marker::PhantomData;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    task::{Context, Poll, Waker},
};

/// A [`NetworkBehaviour`]:
/// - forwards messages to all connected peers with deduplication.
/// - receives messages from all connected peers.
pub struct Behaviour<M> {
    config: Config,
    /// Peers that support the mix protocol, and their connection IDs
    negotiated_peers: HashMap<PeerId, HashSet<ConnectionId>>,
    /// Queue of events to yield to the swarm.
    events: VecDeque<ToSwarm<Event, FromBehaviour>>,
    /// Waker that handles polling
    waker: Option<Waker>,
    /// An LRU time cache for storing seen messages (based on their ID). This cache prevents
    /// duplicates from being propagated on the network.
    duplicate_cache: TimedCache<Vec<u8>, ()>,
    _mix_message: PhantomData<M>,
}

#[derive(Debug)]
pub struct Config {
    pub duplicate_cache_lifespan: u64,
}

#[derive(Debug)]
pub enum Event {
    /// A message received from one of the peers.
    Message(Vec<u8>),
    Error(Error),
}

impl<M> Behaviour<M>
where
    M: MixMessage,
{
    pub fn new(config: Config) -> Self {
        let duplicate_cache = TimedCache::with_lifespan(config.duplicate_cache_lifespan);
        Self {
            config,
            negotiated_peers: HashMap::new(),
            events: VecDeque::new(),
            waker: None,
            duplicate_cache,
            _mix_message: Default::default(),
        }
    }

    /// Publish a message (data or drop) to all connected peers
    pub fn publish(&mut self, message: Vec<u8>) -> Result<(), Error> {
        if M::is_drop_message(&message) {
            // Bypass deduplication for the drop message
            return self.forward_message(message, None);
        }

        let msg_id = Self::message_id(&message);
        // If the message was already seen, don't forward it again
        if self.duplicate_cache.cache_get(&msg_id).is_some() {
            return Ok(());
        }

        let result = self.forward_message(message, None);
        // Add the message to the cache only if the forwarding was successfully triggered
        if result.is_ok() {
            self.duplicate_cache.cache_set(msg_id, ());
        }
        result
    }

    /// Forwards a message to all connected peers except the excluded peer.
    ///
    /// Returns [`Error::NoPeers`] if there are no connected peers that support the mix protocol.
    fn forward_message(
        &mut self,
        message: Vec<u8>,
        excluded_peer: Option<PeerId>,
    ) -> Result<(), Error> {
        let mut peer_ids: HashSet<_> = self.negotiated_peers.keys().collect();
        if let Some(peer) = &excluded_peer {
            peer_ids.remove(peer);
        }

        if peer_ids.is_empty() {
            return Err(Error::NoPeers);
        }

        for peer_id in peer_ids.into_iter() {
            tracing::debug!("Registering event for peer {:?} to send msg", peer_id);
            self.events.push_back(ToSwarm::NotifyHandler {
                peer_id: *peer_id,
                handler: NotifyHandler::Any,
                event: FromBehaviour::Message(message.clone()),
            });
        }

        self.try_wake();
        Ok(())
    }

    fn add_negotiated_peer(&mut self, peer_id: PeerId, connection_id: ConnectionId) -> bool {
        tracing::debug!(
            "Adding to connected_peers: peer_id:{:?}, connection_id:{:?}",
            peer_id,
            connection_id
        );
        self.negotiated_peers
            .entry(peer_id)
            .or_default()
            .insert(connection_id)
    }

    fn remove_negotiated_peer(&mut self, peer_id: &PeerId, connection_id: &ConnectionId) {
        if let Some(connections) = self.negotiated_peers.get_mut(peer_id) {
            tracing::debug!(
                "Removing from connected_peers: peer:{:?}, connection_id:{:?}",
                peer_id,
                connection_id
            );
            connections.remove(connection_id);
            if connections.is_empty() {
                self.negotiated_peers.remove(peer_id);
            }
        }
    }

    /// SHA-256 hash of the message
    fn message_id(message: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(message);
        hasher.finalize().to_vec()
    }

    fn try_wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<M> NetworkBehaviour for Behaviour<M>
where
    M: MixMessage + 'static,
{
    type ConnectionHandler = MixConnectionHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(MixConnectionHandler::new(&self.config))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(MixConnectionHandler::new(&self.config))
    }

    /// Informs the behaviour about an event from the [`Swarm`].
    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            connection_id,
            ..
        }) = event
        {
            self.remove_negotiated_peer(&peer_id, &connection_id);
        }
    }

    /// Handles an event generated by the [`MixConnectionHandler`]
    /// dedicated to the connection identified by `peer_id` and `connection_id`.
    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            // A message was forwarded from the peer.
            ToBehaviour::Message(message) => {
                // Ignore drop message
                if M::is_drop_message(&message) {
                    return;
                }

                // Add the message to the cache. If it was already seen, ignore it.
                if self
                    .duplicate_cache
                    .cache_set(Self::message_id(&message), ())
                    .is_some()
                {
                    return;
                }

                // Forward the message immediately to the rest of connected peers
                // without any processing for the fast propagation.
                if let Err(e) = self.forward_message(message.clone(), Some(peer_id)) {
                    tracing::error!("Failed to forward message: {e:?}");
                }

                // Notify the swarm about the received message,
                // so that it can be processed by the core protocol module.
                self.events
                    .push_back(ToSwarm::GenerateEvent(Event::Message(message)));
            }
            // The connection was fully negotiated by the peer,
            // which means that the peer supports the mix protocol.
            ToBehaviour::FullyNegotiatedOutbound => {
                self.add_negotiated_peer(peer_id, connection_id);
            }
            ToBehaviour::NegotiationFailed => {
                self.remove_negotiated_peer(&peer_id, &connection_id);
            }
            ToBehaviour::IOError(error) => {
                // TODO: Consider removing the peer from the connected_peers and closing the connection
                self.events
                    .push_back(ToSwarm::GenerateEvent(Event::Error(Error::PeerIOError {
                        error,
                        peer_id,
                        connection_id,
                    })));
            }
        }

        self.try_wake();
    }

    /// Polls for things that swarm should do.
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            self.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

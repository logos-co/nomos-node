use crate::{
    error::Error,
    handler::{BlendConnectionHandler, FromBehaviour, ToBehaviour},
};
use cached::{Cached, TimedCache};
use libp2p::{
    core::Endpoint,
    swarm::{
        behaviour::ConnectionEstablished, ConnectionClosed, ConnectionDenied, ConnectionId,
        FromSwarm, NetworkBehaviour, NotifyHandler, THandler, THandlerInEvent, THandlerOutEvent,
        ToSwarm,
    },
    Multiaddr, PeerId,
};
use nomos_blend::conn_maintenance::{ConnectionMonitor, ConnectionMonitorSettings};
use nomos_blend_message::BlendMessage;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashSet, VecDeque},
    task::{Context, Poll, Waker},
};
use std::{marker::PhantomData, time::Duration};

/// A [`NetworkBehaviour`]:
/// - forwards messages to all connected peers with deduplication.
/// - receives messages from all connected peers.
pub struct Behaviour<M, IntervalProvider>
where
    M: BlendMessage,
    IntervalProvider: IntervalStreamProvider,
{
    config: Config,
    peers: HashSet<PeerId>,
    /// Queue of events to yield to the swarm.
    events: VecDeque<ToSwarm<Event, FromBehaviour>>,
    /// Waker that handles polling
    waker: Option<Waker>,
    /// An LRU time cache for storing seen messages (based on their ID). This cache prevents
    /// duplicates from being propagated on the network.
    duplicate_cache: TimedCache<Vec<u8>, ()>,
    _blend_message: PhantomData<M>,
    _interval_provider: PhantomData<IntervalProvider>,
}

#[derive(Debug)]
pub struct Config {
    pub duplicate_cache_lifespan: u64,
    pub conn_monitor_settings: Option<ConnectionMonitorSettings>,
}

#[derive(Debug)]
pub enum Event {
    /// A message received from one of the peers.
    Message(Vec<u8>),
    Error(Error),
}

impl<M, IntervalProvider> Behaviour<M, IntervalProvider>
where
    M: BlendMessage,
    M::PublicKey: PartialEq,
    IntervalProvider: IntervalStreamProvider,
{
    pub fn new(config: Config) -> Self {
        let duplicate_cache = TimedCache::with_lifespan(config.duplicate_cache_lifespan);
        Self {
            config,
            peers: HashSet::new(),
            events: VecDeque::new(),
            waker: None,
            duplicate_cache,
            _blend_message: PhantomData,
            _interval_provider: PhantomData,
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
    /// Returns [`Error::NoPeers`] if there are no connected peers that support the blend protocol.
    fn forward_message(
        &mut self,
        message: Vec<u8>,
        excluded_peer: Option<PeerId>,
    ) -> Result<(), Error> {
        let mut peer_ids = self.peers.clone();
        if let Some(peer) = &excluded_peer {
            peer_ids.remove(peer);
        }

        if peer_ids.is_empty() {
            return Err(Error::NoPeers);
        }

        peer_ids.into_iter().for_each(|peer_id| {
            tracing::debug!("Registering event for peer {:?} to send msg", peer_id);
            self.events.push_back(ToSwarm::NotifyHandler {
                peer_id,
                handler: NotifyHandler::Any,
                event: FromBehaviour::Message(message.clone()),
            });
        });

        self.try_wake();
        Ok(())
    }

    fn message_id(message: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(message);
        hasher.finalize().to_vec()
    }

    fn create_connection_handler(&self) -> BlendConnectionHandler<M> {
        let monitor = self.config.conn_monitor_settings.as_ref().map(|settings| {
            ConnectionMonitor::new(
                *settings,
                IntervalProvider::interval_stream(settings.interval),
            )
        });
        BlendConnectionHandler::new(monitor)
    }

    fn try_wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<M, IntervalProvider> NetworkBehaviour for Behaviour<M, IntervalProvider>
where
    M: BlendMessage + Send + 'static,
    M::PublicKey: PartialEq + 'static,
    IntervalProvider: IntervalStreamProvider + 'static,
{
    type ConnectionHandler = BlendConnectionHandler<M>;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.create_connection_handler())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        _: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(self.create_connection_handler())
    }

    /// Informs the behaviour about an event from the [`Swarm`].
    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished { .. }) => {
                // TODO: Notify the connection handler to deny the stream if necessary
                // - if max peering degree was reached.
                // - if the peer has been detected as malicious.
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                remaining_established,
                ..
            }) => {
                if remaining_established == 0 {
                    self.peers.remove(&peer_id);
                }
            }
            _ => {}
        }
    }

    /// Handles an event generated by the [`BlendConnectionHandler`]
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
                // TODO: move this check into ConnectionHandler since it now has access to the message type
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
            // which means that the peer supports the blend protocol.
            ToBehaviour::FullyNegotiatedOutbound => {
                self.peers.insert(peer_id);
            }
            ToBehaviour::NegotiationFailed => {
                self.peers.remove(&peer_id);
            }
            ToBehaviour::MaliciousPeer => {
                // TODO: Remove the peer from the connected peer list
                // and add it to the malicious peer list,
                // so that the peer is excluded from the future connection establishments.
                // Also, notify the upper layer to try to dial new peers
                // if we need more healthy peers.
            }
            ToBehaviour::UnhealthyPeer => {
                // TODO: Remove the peer from the connected 'healthy' peer list.
                // Also, notify the upper layer to try to dial new peers
                // if we need more healthy peers.
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

pub trait IntervalStreamProvider {
    fn interval_stream(interval: Duration) -> impl futures::Stream<Item = ()> + Send + 'static;
}

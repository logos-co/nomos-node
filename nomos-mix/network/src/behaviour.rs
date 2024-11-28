use crate::{
    error::Error,
    handler::{FromBehaviour, MixConnectionHandler, ToBehaviour},
};
use cached::{Cached, TimedCache};
use futures::Stream;
use libp2p::{
    core::Endpoint,
    swarm::{
        ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        NotifyHandler, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use nomos_mix::conn_maintenance::{ConnectionMaintenance, ConnectionMaintenanceSettings};
use nomos_mix_message::MixMessage;
use sha2::{Digest, Sha256};
use std::marker::PhantomData;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    pin::pin,
    task::{Context, Poll, Waker},
};

/// A [`NetworkBehaviour`]:
/// - forwards messages to all connected peers with deduplication.
/// - receives messages from all connected peers.
pub struct Behaviour<M, Interval> {
    config: Config,
    /// Peers that support the mix protocol, and their connection IDs
    negotiated_peers: HashMap<PeerId, HashSet<ConnectionId>>,
    conn_maintenance: ConnectionMaintenance<PeerId>,
    conn_maintenance_interval: Interval,
    reconnection_count: usize,
    max_reconnection_count: usize,
    peer_addresses: HashMap<PeerId, Multiaddr>,
    /// Peers that are considered malicious by connection monitoring
    /// because they sent more messages than the expected rate
    malicious_peers: HashSet<PeerId>,
    /// Peers that are considered unhealthy by connection monitoring
    /// because they sent less messages than the expected rate
    unhealthy_peers: HashSet<PeerId>,
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
    pub conn_maintenance_settings: ConnectionMaintenanceSettings,
}

#[derive(Debug)]
pub enum Event {
    /// A message received from one of the peers.
    Message(Vec<u8>),
    EstabalishNewConnection {
        excludes: HashSet<Multiaddr>,
    },
    Error(Error),
}

impl<M, Interval> Behaviour<M, Interval>
where
    M: MixMessage,
{
    pub fn new(config: Config, conn_maintenance_interval: Interval) -> Self {
        let duplicate_cache = TimedCache::with_lifespan(config.duplicate_cache_lifespan);
        let conn_maintenance =
            ConnectionMaintenance::<PeerId>::new(config.conn_maintenance_settings);
        Self {
            config,
            negotiated_peers: HashMap::new(),
            conn_maintenance,
            conn_maintenance_interval,
            reconnection_count: 0,
            max_reconnection_count: 5,
            peer_addresses: HashMap::new(),
            malicious_peers: HashSet::new(),
            unhealthy_peers: HashSet::new(),
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

    fn remove_negotiated_peer(&mut self, peer_id: &PeerId, connection_id: Option<&ConnectionId>) {
        match connection_id {
            Some(connection_id) => {
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
            None => {
                self.negotiated_peers.remove(peer_id);
            }
        }
    }

    fn add_malicious_peers(&mut self, peers: HashSet<PeerId>) {
        peers.into_iter().for_each(|peer_id| {
            self.events.push_back(ToSwarm::CloseConnection {
                peer_id,
                connection: libp2p::swarm::CloseConnection::All,
            });
            self.remove_negotiated_peer(&peer_id, None);
            self.malicious_peers.insert(peer_id);
            self.schedule_dial();
        });
    }

    fn add_unhealthy_peers(&mut self, peers: HashSet<PeerId>) {
        peers.into_iter().for_each(|peer_id| {
            if self.unhealthy_peers.insert(peer_id)
                && self.reconnection_count < self.max_reconnection_count
            {
                self.schedule_dial();
                self.reconnection_count += 1;
            }
        });
    }

    fn schedule_dial(&mut self) {
        let mut excludes = HashSet::new();
        self.malicious_peers.iter().for_each(|peer_id| {
            if let Some(addr) = self.peer_addresses.get(peer_id) {
                excludes.insert(addr.clone());
            }
        });
        self.unhealthy_peers.iter().for_each(|peer_id| {
            if let Some(addr) = self.peer_addresses.get(peer_id) {
                excludes.insert(addr.clone());
            }
        });

        self.events
            .push_back(ToSwarm::GenerateEvent(Event::EstabalishNewConnection {
                excludes,
            }));
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

impl<M, Interval> NetworkBehaviour for Behaviour<M, Interval>
where
    M: MixMessage + 'static,
    Interval: Stream + Unpin + 'static,
{
    type ConnectionHandler = MixConnectionHandler;
    type ToSwarm = Event;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer_id: PeerId,
        _: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.peer_addresses.insert(peer_id, remote_addr.clone());
        Ok(MixConnectionHandler::new(&self.config))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer_id: PeerId,
        addr: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.peer_addresses.insert(peer_id, addr.clone());
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
            self.remove_negotiated_peer(&peer_id, Some(&connection_id));
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
                    self.conn_maintenance.add_drop(peer_id);
                    return;
                }

                // TODO: Discuss about the spec again.
                // Due to immediate forwarding (which bypass Persistent Transmission),
                // the measured effective messages may be very higher than the expected.
                self.conn_maintenance.add_effective(peer_id);

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
                self.remove_negotiated_peer(&peer_id, Some(&connection_id));
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
        if pin!(&mut self.conn_maintenance_interval)
            .poll_next(cx)
            .is_ready()
        {
            let (malicious_peers, unhealthy_peers) = self.conn_maintenance.reset();
            self.add_malicious_peers(malicious_peers);
            self.add_unhealthy_peers(unhealthy_peers);
        }

        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            self.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

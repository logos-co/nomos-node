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
    config: Config<Interval>,
    /// Peers that support the mix protocol, and their connection IDs
    negotiated_peers: HashMap<PeerId, HashSet<ConnectionId>>,
    /// Connection maintenance
    /// NOTE: For now, this is optional because this may close too many connections
    /// until we clearly figure out the optimal parameters.
    conn_maintenance: Option<ConnectionMaintenance<PeerId>>,
    /// Peers that should be excluded from connection establishments
    blacklist_peers: HashSet<PeerId>,
    /// To maintain address of peers because libp2p API uses only [`PeerId`] when handling
    /// connection events. But, we need to know [`Multiaddr`] of peers to dial to them.
    peer_addresses: HashMap<PeerId, Multiaddr>,
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
pub struct Config<Interval> {
    pub max_peering_degree: usize,
    pub duplicate_cache_lifespan: u64,
    pub conn_maintenance_settings: Option<ConnectionMaintenanceSettings>,
    pub conn_maintenance_interval: Option<Interval>,
}

#[derive(Debug)]
pub enum Event {
    /// A message received from one of the peers.
    Message(Vec<u8>),
    /// Request establishing the `[amount]` number of new connections,
    /// excluding the peers with the given addresses.
    EstabalishNewConnections {
        amount: usize,
        excludes: HashSet<Multiaddr>,
    },
    Error(Error),
}

impl<M, Interval> Behaviour<M, Interval>
where
    M: MixMessage,
{
    pub fn new(config: Config<Interval>) -> Self {
        let Config {
            conn_maintenance_settings,
            conn_maintenance_interval,
            ..
        } = &config;
        assert_eq!(
            conn_maintenance_settings.is_some(),
            conn_maintenance_interval.is_some()
        );
        let conn_maintenance = conn_maintenance_settings.map(ConnectionMaintenance::<PeerId>::new);
        let duplicate_cache = TimedCache::with_lifespan(config.duplicate_cache_lifespan);
        Self {
            config,
            negotiated_peers: HashMap::new(),
            conn_maintenance,
            peer_addresses: HashMap::new(),
            blacklist_peers: HashSet::new(),
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

    /// Add a peer to the list of connected peers that have completed negotiations
    /// to verify [`Behaviour`] support.
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

    /// Remove a peer from the list of connected peers.
    /// If the peer (or the connection of the peer) is found and removed, return `false`.
    /// If not, return `true`.
    fn remove_negotiated_peer(
        &mut self,
        peer_id: &PeerId,
        connection_id: Option<&ConnectionId>,
    ) -> bool {
        match connection_id {
            Some(connection_id) => match self.negotiated_peers.get_mut(peer_id) {
                Some(connections) => {
                    tracing::debug!(
                        "Removing from connected_peers: peer:{:?}, connection_id:{:?}",
                        peer_id,
                        connection_id
                    );
                    let removed = connections.remove(connection_id);
                    if connections.is_empty() {
                        self.negotiated_peers.remove(peer_id);
                    }
                    removed
                }
                None => false,
            },
            None => self.negotiated_peers.remove(peer_id).is_some(),
        }
    }

    fn is_negotiated_peer(&self, peer_id: &PeerId) -> bool {
        self.negotiated_peers.contains_key(peer_id)
    }

    /// Handle newly detected malicious and unhealthy peers and schedule new connection establishments.
    /// This must be called at the end of each time window.
    fn run_conn_maintenance(&mut self) {
        if let Some(conn_maintenance) = self.conn_maintenance.as_mut() {
            let (malicious_peers, unhealthy_peers) = conn_maintenance.reset();
            let num_closed_malicious = self.handle_malicious_peers(malicious_peers);
            let num_connected_unhealthy = self.handle_unhealthy_peers(unhealthy_peers);
            let mut num_to_dial = num_closed_malicious + num_connected_unhealthy;
            if num_to_dial + self.negotiated_peers.len() > self.config.max_peering_degree {
                tracing::warn!(
                    "Cannot establish {} new connections due to max_peering_degree:{}. connected_peers:{}",
                    num_to_dial,
                    self.config.max_peering_degree,
                    self.negotiated_peers.len(),
                );
                num_to_dial = self.config.max_peering_degree - self.negotiated_peers.len();
            }
            self.schedule_dials(num_to_dial);
        }
    }

    /// Add newly detected malicious peers to the blacklist,
    /// and schedule connection closures if the connections exist.
    /// This returns the number of connection closures that were actually scheduled.
    fn handle_malicious_peers(&mut self, peers: HashSet<PeerId>) -> usize {
        peers
            .into_iter()
            .map(|peer_id| {
                self.blacklist_peers.insert(peer_id);
                // Close the connection only if the peer was already connected.
                if self.remove_negotiated_peer(&peer_id, None) {
                    self.events.push_back(ToSwarm::CloseConnection {
                        peer_id,
                        connection: libp2p::swarm::CloseConnection::All,
                    });
                    1
                } else {
                    0
                }
            })
            .sum()
    }

    /// Add newly detected unhealthy peers to the blacklist,
    /// and return the number of connections that actually exist to the unhealthy peers.
    fn handle_unhealthy_peers(&mut self, peers: HashSet<PeerId>) -> usize {
        peers
            .into_iter()
            .map(|peer_id| {
                self.blacklist_peers.insert(peer_id);
                if self.is_negotiated_peer(&peer_id) {
                    1
                } else {
                    0
                }
            })
            .sum()
    }

    /// Schedule new connection establishments, excluding the blacklisted peers.
    fn schedule_dials(&mut self, amount: usize) {
        if amount == 0 {
            return;
        }

        let excludes = self
            .blacklist_peers
            .iter()
            .filter_map(|peer_id| self.peer_addresses.get(peer_id).cloned())
            .collect();
        tracing::info!(
            "Scheduling {} new dialings: excludes:{:?}",
            amount,
            excludes
        );
        self.events
            .push_back(ToSwarm::GenerateEvent(Event::EstabalishNewConnections {
                amount,
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
        // Keep the address of the peer
        self.peer_addresses.insert(peer_id, remote_addr.clone());
        Ok(MixConnectionHandler::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer_id: PeerId,
        addr: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        // Keep the address of the peer
        self.peer_addresses.insert(peer_id, addr.clone());
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
                    if let Some(conn_maintenance) = self.conn_maintenance.as_mut() {
                        conn_maintenance.add_drop(peer_id);
                    }
                    return;
                }

                if let Some(conn_maintenance) = self.conn_maintenance.as_mut() {
                    conn_maintenance.add_effective(peer_id);
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
        // Run connection maintenance if the interval is reached.
        if let Some(interval) = self.config.conn_maintenance_interval.as_mut() {
            if pin!(interval).poll_next(cx).is_ready() {
                self.run_conn_maintenance();
            }
        }

        if let Some(event) = self.events.pop_front() {
            Poll::Ready(event)
        } else {
            self.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

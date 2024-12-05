use crate::{
    error::Error,
    handler::{FromBehaviour, MixConnectionHandler, ToBehaviour},
};
use cached::{Cached, TimedCache};
use futures::Stream;
use libp2p::{
    core::Endpoint,
    swarm::{
        dial_opts::DialOpts, CloseConnection, ConnectionClosed, ConnectionDenied, ConnectionId,
        FromSwarm, NetworkBehaviour, NotifyHandler, THandler, THandlerInEvent, THandlerOutEvent,
        ToSwarm,
    },
    Multiaddr, PeerId,
};
use nomos_mix::{
    conn_maintenance::{ConnectionMaintenance, ConnectionMaintenanceSettings},
    membership::Membership,
};
use nomos_mix_message::MixMessage;
use rand::RngCore;
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
pub struct Behaviour<M, R, Interval>
where
    M: MixMessage,
    R: RngCore,
{
    config: Config<Interval>,
    /// Connection maintenance
    conn_maintenance: ConnectionMaintenance<M, R>,
    peer_address_map: PeerAddressMap,
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
    pub duplicate_cache_lifespan: u64,
    pub conn_maintenance_settings: ConnectionMaintenanceSettings,
    pub conn_maintenance_interval: Option<Interval>,
}

#[derive(Debug)]
pub enum Event {
    /// A message received from one of the peers.
    Message(Vec<u8>),
    Error(Error),
}

impl<M, R, Interval> Behaviour<M, R, Interval>
where
    M: MixMessage,
    M::PublicKey: PartialEq,
    R: RngCore,
{
    pub fn new(config: Config<Interval>, membership: Membership<M>, rng: R) -> Self {
        let mut conn_maintenance =
            ConnectionMaintenance::<M, R>::new(config.conn_maintenance_settings, membership, rng);

        // Bootstrap connections with initial peers randomly chosen.
        let peer_addrs: Vec<Multiaddr> = conn_maintenance.bootstrap();
        let events = peer_addrs
            .into_iter()
            .map(|peer_addr| ToSwarm::Dial {
                opts: DialOpts::from(peer_addr),
            })
            .collect::<VecDeque<_>>();

        let duplicate_cache = TimedCache::with_lifespan(config.duplicate_cache_lifespan);
        Self {
            config,
            conn_maintenance,
            peer_address_map: PeerAddressMap::new(),
            events,
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
        let mut peer_ids = self
            .conn_maintenance
            .connected_peers()
            .iter()
            .filter_map(|addr| self.peer_address_map.peer_id(addr))
            .collect::<HashSet<_>>();
        if let Some(peer) = &excluded_peer {
            peer_ids.remove(peer);
        }

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

        self.try_wake();
        Ok(())
    }

    fn message_id(message: &[u8]) -> Vec<u8> {
        let mut hasher = Sha256::new();
        hasher.update(message);
        hasher.finalize().to_vec()
    }

    fn run_conn_maintenance(&mut self) {
        if let Some((_, peers_to_close, peers_to_connect)) = self.conn_maintenance.reset() {
            // Schedule events to close connections
            peers_to_close.into_iter().for_each(|addr| {
                if let Some(peer_id) = self.peer_address_map.peer_id(&addr) {
                    self.events.push_back(ToSwarm::CloseConnection {
                        peer_id,
                        connection: CloseConnection::All,
                    });
                }
            });
            // Schedule events to connect to peers
            peers_to_connect.into_iter().for_each(|addr| {
                self.events.push_back(ToSwarm::Dial {
                    opts: DialOpts::from(addr),
                });
            });
        }
    }

    fn try_wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<M, R, Interval> NetworkBehaviour for Behaviour<M, R, Interval>
where
    M: MixMessage + 'static,
    M::PublicKey: PartialEq + 'static,
    R: RngCore + 'static,
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
        // Keep PeerId <> Multiaddr mapping
        self.peer_address_map.add(peer_id, remote_addr.clone());
        Ok(MixConnectionHandler::new())
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer_id: PeerId,
        addr: &Multiaddr,
        _: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        // Keep PeerId <> Multiaddr mapping
        self.peer_address_map.add(peer_id, addr.clone());
        Ok(MixConnectionHandler::new())
    }

    /// Informs the behaviour about an event from the [`Swarm`].
    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            remaining_established,
            ..
        }) = event
        {
            if remaining_established == 0 {
                if let Some(addr) = self.peer_address_map.address(&peer_id) {
                    self.conn_maintenance.remove_connected_peer(addr);
                }
            }
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
                    if let Some(addr) = self.peer_address_map.address(&peer_id) {
                        self.conn_maintenance.record_drop_message(addr);
                    }
                    return;
                }

                if let Some(addr) = self.peer_address_map.address(&peer_id) {
                    self.conn_maintenance.record_effective_message(addr);
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
                if let Some(addr) = self.peer_address_map.address(&peer_id) {
                    self.conn_maintenance.add_connected_peer(addr.clone());
                }
            }
            ToBehaviour::NegotiationFailed => {
                if let Some(addr) = self.peer_address_map.address(&peer_id) {
                    self.conn_maintenance.remove_connected_peer(addr);
                }
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
        if let Some(interval) = &mut self.config.conn_maintenance_interval {
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

struct PeerAddressMap {
    peer_to_addr: HashMap<PeerId, Multiaddr>,
    addr_to_peer: HashMap<Multiaddr, PeerId>,
}

impl PeerAddressMap {
    fn new() -> Self {
        Self {
            peer_to_addr: HashMap::new(),
            addr_to_peer: HashMap::new(),
        }
    }

    fn add(&mut self, peer_id: PeerId, addr: Multiaddr) {
        self.peer_to_addr.insert(peer_id, addr.clone());
        self.addr_to_peer.insert(addr, peer_id);
    }

    fn peer_id(&self, addr: &Multiaddr) -> Option<PeerId> {
        self.addr_to_peer.get(addr).copied()
    }

    fn address(&self, peer_id: &PeerId) -> Option<&Multiaddr> {
        self.peer_to_addr.get(peer_id)
    }
}

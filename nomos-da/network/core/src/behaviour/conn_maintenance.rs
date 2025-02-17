// std
use std::{
    collections::{HashSet, VecDeque},
    convert::Infallible,
    fmt,
    task::{Context, Poll, Waker},
};
// crates
use libp2p::{
    core::{transport::PortUse, Endpoint},
    swarm::{
        dummy, CloseConnection, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
// internal

pub trait ConnectionMonitor {}

/// A `NetworkBehaviour` that maintains consistent number of  connections to DA subnetwork nodes.
#[derive(Default, Debug)]
pub struct ConnectionMaintenanceBehaviour<Monitor> {
    monitor: Monitor,
    blocked_peers: HashSet<PeerId>,
    open_connections: VecDeque<PeerId>,
    close_connections: VecDeque<PeerId>,
    waker: Option<Waker>,
}

impl<Monitor: ConnectionMonitor> ConnectionMaintenanceBehaviour<Monitor> {
    /// Peers that are currently blocked.
    pub fn blocked_peers(&self) -> &HashSet<PeerId> {
        &self.blocked_peers
    }

    /// Block connections to a given peer.
    ///
    /// All active connections to this peer will be closed immediately.
    ///
    /// Returns whether the peer was newly inserted. Does nothing if the peer was already present in
    /// the set.
    pub fn block_peer(&mut self, peer: PeerId) -> bool {
        let inserted = self.blocked_peers.insert(peer);
        if inserted {
            self.close_connections.push_back(peer);
            if let Some(waker) = self.waker.take() {
                waker.wake()
            }
        }
        inserted
    }

    /// Unblock connections to a given peer.
    ///
    /// Returns whether the peer was present in the set. Does nothing if the peer
    /// was not present in the set.
    pub fn unblock_peer(&mut self, peer: PeerId) -> bool {
        let removed = self.blocked_peers.remove(&peer);
        if removed {
            if let Some(waker) = self.waker.take() {
                waker.wake()
            }
        }
        removed
    }
}

/// A connection to this peer was explicitly blocked and was thus [`denied`](ConnectionDenied).
#[derive(Debug)]
pub struct Blocked {
    peer: PeerId,
}

impl fmt::Display for Blocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "peer {} is in the block list", self.peer)
    }
}

impl std::error::Error for Blocked {}

trait Enforce {
    fn enforce(&self, peer: &PeerId) -> Result<(), ConnectionDenied>;
}

impl<Monitor> Enforce for ConnectionMaintenanceBehaviour<Monitor> {
    fn enforce(&self, peer: &PeerId) -> Result<(), ConnectionDenied> {
        if self.blocked_peers.contains(peer) {
            return Err(ConnectionDenied::new(Blocked { peer: *peer }));
        }

        Ok(())
    }
}

impl<Monitor> NetworkBehaviour for ConnectionMaintenanceBehaviour<Monitor>
where
    Monitor: 'static,
{
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.enforce(&peer)?;

        Ok(dummy::ConnectionHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: Option<PeerId>,
        _: &[Multiaddr],
        _: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        if let Some(peer) = peer {
            self.enforce(&peer)?;
        }

        Ok(vec![])
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.enforce(&peer)?;

        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, _event: FromSwarm) {}

    fn on_connection_handler_event(
        &mut self,
        _id: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        // TODO: remove when Rust 1.82 is MSRV
        #[allow(unreachable_patterns)]
        libp2p::core::util::unreachable(event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(peer) = self.close_connections.pop_front() {
            return Poll::Ready(ToSwarm::CloseConnection {
                peer_id: peer,
                connection: CloseConnection::All,
            });
        }

        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

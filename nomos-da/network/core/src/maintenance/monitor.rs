// std
use std::{
    collections::{HashMap, HashSet, VecDeque},
    convert::Infallible,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
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
use thiserror::Error;
// internal

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PeerStatus {
    Malicious,
    Unhealthy,
    Healthy,
}

pub struct ConnectionMonitorOutput {
    pub peer_id: PeerId,
    pub peer_status: PeerStatus,
}

pub trait ConnectionMonitor {
    type Event;

    fn record_event(&mut self, event: Self::Event) -> Option<ConnectionMonitorOutput>;
    fn reset_peer(&mut self, peer_id: &PeerId);
}

/// A `NetworkBehaviour` that maintains consistent number of  connections to DA subnetwork nodes.
#[derive(Debug)]
pub struct ConnectionMonitorBehaviour<Monitor> {
    monitor: Monitor,
    malicous_peers: HashSet<PeerId>,
    unhealthy_peers: HashMap<PeerId, Instant>,
    close_connections: VecDeque<PeerId>,
    redial_cooldown: Duration,
    waker: Option<Waker>,
}

impl<Monitor> ConnectionMonitorBehaviour<Monitor>
where
    Monitor: ConnectionMonitor,
{
    pub fn new(monitor: Monitor, redial_cooldown: Duration) -> Self {
        Self {
            monitor,
            malicous_peers: HashSet::new(),
            unhealthy_peers: HashMap::new(),
            close_connections: VecDeque::new(),
            redial_cooldown,
            waker: None,
        }
    }

    /// Block connections to a given peer.
    ///
    /// All active connections to this peer will be closed immediately.
    ///
    /// Returns whether the peer was newly inserted. Does nothing if the peer was already present in
    /// the set.
    pub fn block_peer(&mut self, peer: PeerId) -> bool {
        let inserted = self.malicous_peers.insert(peer);
        if inserted {
            self.close_connections.push_back(peer);
            if let Some(waker) = self.waker.take() {
                waker.wake()
            }
        }
        inserted
    }

    /// **Temporarily blocks a peer** due to unhealthy behavior.
    ///
    /// The peer is added to `unhealthy_peers`, and after `redial_cooldown`, it will be allowed to reconnect.
    /// The existing connections to this peer are **closed immediately**.
    pub fn temporarily_block_peer(&mut self, peer: PeerId) {
        let until = Instant::now() + self.redial_cooldown;
        self.unhealthy_peers.insert(peer, until);
        // Close existing connections
        self.close_connections.push_back(peer);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }

    /// Unblock connections to a given peer.
    ///
    /// Returns whether the peer was present in the set. Does nothing if the peer
    /// was not present in the set.
    pub fn unblock_peer(&mut self, peer: PeerId) -> bool {
        let removed =
            self.malicous_peers.remove(&peer) || self.unhealthy_peers.remove(&peer).is_some();
        if removed {
            if let Some(waker) = self.waker.take() {
                waker.wake()
            }
        }
        removed
    }

    pub fn record_event(&mut self, event: Monitor::Event) {
        if let Some(output) = self.monitor.record_event(event) {
            match output.peer_status {
                PeerStatus::Malicious => {
                    self.block_peer(output.peer_id);
                }
                PeerStatus::Unhealthy => {
                    self.temporarily_block_peer(output.peer_id);
                }
                _ => {}
            }
        }
    }

    /// Enforce connection rules (deny blocked peers).
    fn enforce(&mut self, peer: &PeerId) -> Result<(), ConnectionDenied> {
        let now = Instant::now();
        self.unhealthy_peers
            .retain(|_peer, &mut until| now <= until);

        if self.malicous_peers.contains(peer) {
            return Err(ConnectionDenied::new(Blocked { peer: *peer }));
        }
        if self.unhealthy_peers.contains_key(peer) {
            return Err(ConnectionDenied::new(TemporarilyBlocked { peer: *peer }));
        }
        Ok(())
    }
}

/// A connection to this peer was explicitly blocked or malicious.
#[derive(Debug, Error)]
#[error("peer {peer} is in the block list")]
pub struct Blocked {
    peer: PeerId,
}

/// A connection to this peer is temporarily blocked due to being unhealthy.
#[derive(Debug, Error)]
#[error("peer {peer} is temporarily blocked due to being unhealthy")]
pub struct TemporarilyBlocked {
    peer: PeerId,
}

impl<Monitor> NetworkBehaviour for ConnectionMonitorBehaviour<Monitor>
where
    Monitor: ConnectionMonitor + 'static,
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

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use libp2p::{
        swarm::{dial_opts::DialOpts, DialError, ListenError, SwarmEvent},
        PeerId, Swarm,
    };
    use libp2p_swarm_test::SwarmExt;

    use crate::maintenance::monitor::{
        Blocked, ConnectionMonitor, ConnectionMonitorBehaviour, ConnectionMonitorOutput,
        PeerStatus, TemporarilyBlocked,
    };

    #[derive(Default)]
    struct MockMonitor {
        stats: HashMap<PeerId, PeerStatus>,
    }

    impl ConnectionMonitor for MockMonitor {
        type Event = (PeerId, PeerStatus);

        fn record_event(
            &mut self,
            (peer_id, peer_status): Self::Event,
        ) -> Option<ConnectionMonitorOutput> {
            self.stats.insert(peer_id, peer_status);
            Some(ConnectionMonitorOutput {
                peer_id,
                peer_status,
            })
        }

        fn reset_peer(&mut self, peer_id: &PeerId) {
            self.stats.remove(peer_id);
        }
    }

    #[tokio::test]
    async fn test_cannot_dial_unhealthy_peer() {
        let mut dialer = Swarm::new_ephemeral(|_| {
            ConnectionMonitorBehaviour::new(MockMonitor::default(), Duration::from_secs(1))
        });
        let mut listener = Swarm::new_ephemeral(|_| {
            ConnectionMonitorBehaviour::new(MockMonitor::default(), Duration::from_secs(1))
        });
        listener.listen().with_memory_addr_external().await;

        let listener_peer = *listener.local_peer_id();
        dialer
            .behaviour_mut()
            .record_event((listener_peer, PeerStatus::Unhealthy));

        let DialError::Denied { cause } = dial(&mut dialer, &listener).unwrap_err() else {
            panic!("unexpected dial error")
        };
        assert!(cause.downcast::<TemporarilyBlocked>().is_ok());
    }

    #[tokio::test]
    async fn test_can_dial_unhealthy_peer_after_cooldown() {
        let mut dialer = Swarm::new_ephemeral(|_| {
            ConnectionMonitorBehaviour::new(MockMonitor::default(), Duration::from_millis(100))
        });
        let mut listener = Swarm::new_ephemeral(|_| {
            ConnectionMonitorBehaviour::new(MockMonitor::default(), Duration::from_millis(100))
        });
        listener.listen().with_memory_addr_external().await;

        let listener_peer = *listener.local_peer_id();

        dialer
            .behaviour_mut()
            .record_event((listener_peer, PeerStatus::Unhealthy));

        let DialError::Denied { cause } = dial(&mut dialer, &listener).unwrap_err() else {
            panic!("unexpected dial error")
        };
        assert!(cause.downcast::<TemporarilyBlocked>().is_ok());

        tokio::time::sleep(Duration::from_millis(150)).await;

        // Attempt to dial again (should succeed)
        assert!(dial(&mut dialer, &listener).is_ok());
    }

    #[tokio::test]
    async fn test_cannot_accept_malicious_peer() {
        let mut dialer = Swarm::new_ephemeral(|_| {
            ConnectionMonitorBehaviour::new(MockMonitor::default(), Duration::ZERO)
        });
        let mut listener = Swarm::new_ephemeral(|_| {
            ConnectionMonitorBehaviour::new(MockMonitor::default(), Duration::ZERO)
        });
        listener.listen().with_memory_addr_external().await;

        let dialer_peer = *dialer.local_peer_id();
        listener
            .behaviour_mut()
            .record_event((dialer_peer, PeerStatus::Malicious));

        dial(&mut dialer, &listener).unwrap();
        tokio::spawn(dialer.loop_on_next());

        let cause = listener
            .wait(|e| match e {
                SwarmEvent::IncomingConnectionError {
                    error: ListenError::Denied { cause },
                    ..
                } => Some(cause),
                _ => None,
            })
            .await;
        assert!(cause.downcast::<Blocked>().is_ok());
    }

    fn dial(
        dialer: &mut Swarm<ConnectionMonitorBehaviour<MockMonitor>>,
        listener: &Swarm<ConnectionMonitorBehaviour<MockMonitor>>,
    ) -> Result<(), DialError> {
        dialer.dial(
            DialOpts::peer_id(*listener.local_peer_id())
                .addresses(listener.external_addresses().cloned().collect())
                .build(),
        )
    }
}

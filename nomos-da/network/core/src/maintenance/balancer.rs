use std::{
    collections::VecDeque,
    convert::Infallible,
    task::{Context, Poll, Waker},
};

use libp2p::{
    core::{
        transport::PortUse,
        ConnectedPoint::{Dialer, Listener},
        Endpoint,
    },
    swarm::{
        dial_opts::DialOpts, dummy, ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm,
        NetworkBehaviour, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};

use crate::address_book::AddressBook;

pub enum ConnectionEvent {
    OpenInbound(PeerId),
    OpenOutbound(PeerId),
    CloseInbound(PeerId),
    CloseOutbound(PeerId),
}

pub trait ConnectionBalancer {
    fn record_event(&mut self, event: ConnectionEvent);
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<VecDeque<PeerId>>;
}

pub struct ConnectionBalancerBehaviour<Balancer> {
    addresses: AddressBook,
    balancer: Balancer,
    peers_to_dial: VecDeque<PeerId>,
    waker: Option<Waker>,
}

impl<Balancer> ConnectionBalancerBehaviour<Balancer>
where
    Balancer: ConnectionBalancer,
{
    pub fn new(addresses: AddressBook, balancer: Balancer) -> Self {
        Self {
            addresses,
            balancer,
            peers_to_dial: VecDeque::new(),
            waker: None,
        }
    }

    pub fn update_addresses(&mut self, addresses: AddressBook) {
        self.addresses = addresses;
    }

    fn record_event(&mut self, event: ConnectionEvent) {
        self.balancer.record_event(event);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<Balancer> NetworkBehaviour for ConnectionBalancerBehaviour<Balancer>
where
    Balancer: ConnectionBalancer + 'static,
{
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: &Multiaddr,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.record_event(ConnectionEvent::OpenInbound(peer));
        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        peer: PeerId,
        _: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<Self::ConnectionHandler, ConnectionDenied> {
        self.record_event(ConnectionEvent::OpenOutbound(peer));
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id, endpoint, ..
        }) = event
        {
            match endpoint {
                Dialer { .. } => {
                    self.record_event(ConnectionEvent::CloseInbound(peer_id));
                }
                Listener { .. } => {
                    self.record_event(ConnectionEvent::CloseOutbound(peer_id));
                }
            }
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _: PeerId,
        _: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        libp2p::core::util::unreachable(event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if self.peers_to_dial.is_empty() {
            if let Poll::Ready(peers) = self.balancer.poll(cx) {
                self.peers_to_dial = peers;
            }
        }

        if let Some(peer) = self.peers_to_dial.pop_front() {
            if let Some(addr) = self.addresses.get_address(&peer) {
                return Poll::Ready(ToSwarm::Dial {
                    opts: DialOpts::peer_id(peer)
                        .addresses(vec![addr.clone()])
                        .build(),
                });
            }
        }

        self.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashSet, VecDeque},
        time::Duration,
    };

    use libp2p::{
        swarm::{Swarm, SwarmEvent},
        PeerId,
    };
    use libp2p_swarm_test::SwarmExt;
    use tokio::time::timeout;

    use super::*;

    #[derive(Default)]
    struct MockBalancer {
        peers_to_connect: VecDeque<PeerId>,
        connected_peers: HashSet<PeerId>,
    }

    impl MockBalancer {
        fn peer_to_connect(&mut self, peer: PeerId) {
            self.peers_to_connect.push_back(peer);
        }
    }

    impl ConnectionBalancer for MockBalancer {
        fn record_event(&mut self, event: ConnectionEvent) {
            match event {
                ConnectionEvent::OpenInbound(peer) | ConnectionEvent::OpenOutbound(peer) => {
                    self.connected_peers.insert(peer);
                }
                ConnectionEvent::CloseInbound(peer) | ConnectionEvent::CloseOutbound(peer) => {
                    self.connected_peers.remove(&peer);
                }
            }
        }

        fn poll(&mut self, _: &mut Context<'_>) -> Poll<VecDeque<PeerId>> {
            if self.peers_to_connect.is_empty() {
                Poll::Pending
            } else {
                Poll::Ready(self.peers_to_connect.drain(..).collect())
            }
        }
    }

    #[tokio::test]
    async fn test_balancer_dials_provided_peers() {
        let mut dialer = Swarm::new_ephemeral(|_| {
            ConnectionBalancerBehaviour::new(AddressBook::empty(), MockBalancer::default())
        });

        let mut listener = Swarm::new_ephemeral(|_| {
            ConnectionBalancerBehaviour::new(AddressBook::empty(), MockBalancer::default())
        });

        let dialer_peer = *dialer.local_peer_id();
        let listener_peer = *listener.local_peer_id();
        listener.listen().with_memory_addr_external().await;

        let address_book = AddressBook::from_iter(
            listener
                .external_addresses()
                .cloned()
                .map(|addr| (listener_peer, addr)),
        );

        dialer.behaviour_mut().update_addresses(address_book);
        // Using balancer `peer_to_connect` we are populating the peers list that will
        // be returned when the balancer is polled by the dialer.
        dialer
            .behaviour_mut()
            .balancer
            .peer_to_connect(listener_peer);

        let listener_task = tokio::spawn(timeout(Duration::from_millis(500), async move {
            listener
                .wait(|e| match e {
                    SwarmEvent::ConnectionEstablished { .. } => Some(()),
                    _ => None,
                })
                .await;

            listener.behaviour().balancer.connected_peers.clone()
        }));

        let dialer_task = tokio::spawn(timeout(Duration::from_millis(500), async move {
            dialer
                .wait(|e| match e {
                    SwarmEvent::ConnectionEstablished { .. } => Some(()),
                    _ => None,
                })
                .await;

            dialer.behaviour().balancer.connected_peers.clone()
        }));

        let (listener_result, dialer_result) = tokio::join!(listener_task, dialer_task);
        let listener_addresses = listener_result
            .expect("Listener timeout")
            .expect("Listener error");
        let dialer_addresses = dialer_result
            .expect("Dialer timeout")
            .expect("Dialer error");

        assert!(listener_addresses.contains(&dialer_peer));
        assert!(dialer_addresses.contains(&listener_peer));
    }

    #[tokio::test]
    async fn test_balancer_dials_all_peers_from_poll() {
        let mut dialer = Swarm::new_ephemeral(|_| {
            ConnectionBalancerBehaviour::new(AddressBook::empty(), MockBalancer::default())
        });

        let peer1 = PeerId::random();
        let peer2 = PeerId::random();
        let peer3 = PeerId::random();

        let address_book = AddressBook::from_iter(vec![
            (peer1, "/ip4/127.0.0.1/tcp/4001".parse().unwrap()),
            (peer2, "/ip4/127.0.0.1/tcp/4002".parse().unwrap()),
            (peer3, "/ip4/127.0.0.1/tcp/4003".parse().unwrap()),
        ]);

        dialer.behaviour_mut().update_addresses(address_book);

        dialer.behaviour_mut().balancer.peer_to_connect(peer1);
        dialer.behaviour_mut().balancer.peer_to_connect(peer2);
        dialer.behaviour_mut().balancer.peer_to_connect(peer3);

        let mut dial_requests = Vec::new();

        let mut cx = Context::from_waker(futures::task::noop_waker_ref());

        // Call poll repeatedly and collect all dial requests
        while let Poll::Ready(to_swarm) = dialer.behaviour_mut().poll(&mut cx) {
            if let ToSwarm::Dial { opts } = to_swarm {
                dial_requests.push(opts);
            }
        }

        let dialed_peers: HashSet<_> = dial_requests
            .iter()
            .map(|opts| opts.get_peer_id().unwrap())
            .collect();

        assert_eq!(dialed_peers.len(), 3, "Expected 3 ToSwarm::Dial requests");
        assert!(
            dialed_peers.contains(&peer1),
            "Expected dial request for peer1"
        );
        assert!(
            dialed_peers.contains(&peer2),
            "Expected dial request for peer2"
        );
        assert!(
            dialed_peers.contains(&peer3),
            "Expected dial request for peer3"
        );

        // Ensure that the balancer queue is now empty
        assert!(dialer.behaviour_mut().balancer.peers_to_connect.is_empty());
    }
}

// std
use std::{
    collections::VecDeque,
    convert::Infallible,
    task::{Context, Poll},
};
// crates
use libp2p::{
    core::{transport::PortUse, Endpoint},
    swarm::{
        dial_opts::DialOpts, dummy, ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm,
        NetworkBehaviour, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
// internal
use crate::address_book::AddressBook;

pub enum ConnectionEvent {
    Inbound(PeerId),
    Outbound(PeerId),
    Close(PeerId),
}

pub trait ConnectionBalancer {
    fn record_event(&mut self, event: ConnectionEvent);
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<VecDeque<PeerId>>;
}

pub struct ConnectionBalancerBehaviour<Balancer> {
    addresses: AddressBook,
    balancer: Balancer,
}

impl<Balancer> ConnectionBalancerBehaviour<Balancer> {
    pub fn new(addresses: AddressBook, balancer: Balancer) -> Self {
        Self {
            addresses,
            balancer,
        }
    }

    pub fn update_addresses(&mut self, addresses: AddressBook) {
        self.addresses = addresses;
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
        self.balancer.record_event(ConnectionEvent::Inbound(peer));
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
        self.balancer.record_event(ConnectionEvent::Outbound(peer));
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed { peer_id, .. }) = event {
            self.balancer.record_event(ConnectionEvent::Close(peer_id));
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
        if let Poll::Ready(mut peers) = self.balancer.poll(cx) {
            while let Some(peer) = peers.pop_front() {
                if let Some(addr) = self.addresses.get_address(&peer) {
                    return Poll::Ready(ToSwarm::Dial {
                        opts: DialOpts::peer_id(peer)
                            .addresses(vec![addr.clone()])
                            .build(),
                    });
                }
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::{
        swarm::{Swarm, SwarmEvent},
        PeerId,
    };
    use libp2p_swarm_test::SwarmExt;
    use std::{
        collections::{HashSet, VecDeque},
        time::Duration,
    };
    use tokio::time::timeout;

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
                ConnectionEvent::Inbound(peer) | ConnectionEvent::Outbound(peer) => {
                    self.connected_peers.insert(peer);
                }
                ConnectionEvent::Close(peer) => {
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
        // Using balancer `peer_to_connect` we are populating the peers list that will be
        // returned when the balancer is polled by the dialer.
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
}

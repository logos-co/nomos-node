use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use futures::StreamExt;
use libp2p::{
    core::upgrade,
    gossipsub, identity, noise,
    swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp, yamux, PeerId, Swarm, Transport,
};
use overwatch_rs::services::state::NoState;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};

use super::NetworkBackend;

const BROADCAST_CHANNEL_BUF: usize = 16;
const MPSC_CHANNEL_BUF: usize = 16;

pub struct Libp2p {
    network_event_tx: broadcast::Sender<NetworkEvent>,
    backend_message_tx: mpsc::Sender<Libp2pBackendMessage>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Libp2pConfig {}

#[derive(Debug)]
pub enum Libp2pBackendMessage {
    //TODO: add Publish, Subscribe, and so on...
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    //TODO: something like WakuMessage
}

struct SwarmHandle {
    swarm: Swarm<Behaviour>,
    network_event_tx: broadcast::Sender<NetworkEvent>,
    backend_message_rx: mpsc::Receiver<Libp2pBackendMessage>,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
}

#[async_trait::async_trait]
impl NetworkBackend for Libp2p {
    type Settings = Libp2pConfig;
    type State = NoState<Libp2pConfig>;
    type Message = Libp2pBackendMessage;
    type EventKind = EventKind;
    type NetworkEvent = NetworkEvent;

    fn new(config: Self::Settings, runtime_handle: tokio::runtime::Handle) -> Self {
        let id_keys = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(id_keys.public());
        tracing::info!("libp2p peer_id:{}", local_peer_id);

        let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1Lazy)
            .authenticate(
                noise::Config::new(&id_keys).expect("signing libp2p-noise static keypair"),
            )
            .multiplex(yamux::Config::default())
            .timeout(std::time::Duration::from_secs(20))
            .boxed();

        let gossipsub_message_id_fn = |message: &gossipsub::Message| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            gossipsub::MessageId::from(s.finish().to_string())
        };

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id_keys),
            gossipsub::ConfigBuilder::default()
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(gossipsub_message_id_fn)
                .build()
                .expect("valid libp2p-gossipsub config"),
        )
        .expect("valid libp2p-gossipsub behaviour config");

        let mut swarm = SwarmBuilder::with_tokio_executor(
            tcp_transport,
            Behaviour { gossipsub },
            local_peer_id,
        )
        .build();

        // TODO: use a proper port
        swarm
            .listen_on("/ip4/0.0.0.0/tcp/0".parse().expect("valid multiaddr"))
            .unwrap();

        let (network_event_tx, _) = broadcast::channel(BROADCAST_CHANNEL_BUF);
        let (backend_message_tx, backend_message_rx) =
            mpsc::channel::<Libp2pBackendMessage>(MPSC_CHANNEL_BUF);

        let swarm_handle = SwarmHandle {
            swarm,
            network_event_tx: network_event_tx.clone(),
            backend_message_rx,
        };
        runtime_handle.spawn(async move { swarm_handle.run().await });

        Self {
            network_event_tx,
            backend_message_tx,
        }
    }

    async fn process(&self, msg: Self::Message) {
        if let Err(e) = self.backend_message_tx.send(msg).await {
            tracing::error!("failed to send backend message to swarm handle: {e}");
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Message => {
                tracing::debug!("processed subscription to incoming messages");
                self.network_event_tx.subscribe()
            }
        }
    }
}

impl SwarmHandle {
    async fn run(mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }
                Some(message) = self.backend_message_rx.recv() => {
                    self.handle_backend_message(message).await;
                }
            }
        }
    }

    async fn handle_swarm_event<T>(&mut self, event: SwarmEvent<BehaviourEvent, T>) {
        //TODO: handle swarm events and return NetworkEvent via channel
        tracing::debug!(
            "Handling swarm event. Network event will be sent via {:?}",
            self.network_event_tx
        );

        match event {
            SwarmEvent::Behaviour(_) => todo!(),
            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                concurrent_dial_errors,
                established_in,
            } => todo!(),
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                endpoint,
                num_established,
                cause,
            } => todo!(),
            SwarmEvent::IncomingConnection {
                connection_id,
                local_addr,
                send_back_addr,
            } => todo!(),
            SwarmEvent::IncomingConnectionError {
                connection_id,
                local_addr,
                send_back_addr,
                error,
            } => todo!(),
            SwarmEvent::OutgoingConnectionError {
                connection_id,
                peer_id,
                error,
            } => todo!(),
            SwarmEvent::NewListenAddr {
                listener_id,
                address,
            } => todo!(),
            SwarmEvent::ExpiredListenAddr {
                listener_id,
                address,
            } => todo!(),
            SwarmEvent::ListenerClosed {
                listener_id,
                addresses,
                reason,
            } => todo!(),
            SwarmEvent::ListenerError { listener_id, error } => todo!(),
            SwarmEvent::Dialing {
                peer_id,
                connection_id,
            } => todo!(),
        }
    }

    async fn handle_backend_message(&mut self, message: Libp2pBackendMessage) {
        tracing::debug!("backend message received: {:?}", message);
        todo!()
    }
}

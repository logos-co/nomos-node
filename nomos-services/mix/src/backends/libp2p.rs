use std::{io, time::Duration};

use async_trait::async_trait;
use futures::StreamExt;
use libp2p::{
    core::transport::ListenerId,
    identity::{ed25519, Keypair},
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm, SwarmBuilder, TransportError,
};
use nomos_libp2p::{secret_key_serde, DialError, DialOpts, Protocol};
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

use super::NetworkBackend;

pub struct Libp2pNetworkBackend {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    msgs_tx: mpsc::Sender<Libp2pNetworkBackendMessage>,
    events_tx: broadcast::Sender<Libp2pNetworkBackendEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Libp2pNetworkBackendSettings {
    pub listening_address: Multiaddr,
    #[serde(with = "secret_key_serde", default = "ed25519::SecretKey::generate")]
    pub node_key: ed25519::SecretKey,
    pub membership: Vec<Multiaddr>,
    pub peering_degree: usize,
    pub num_mix_layers: usize,
}

#[derive(Debug)]
pub enum Libp2pNetworkBackendMessage {
    Mix(Vec<u8>),
}

#[derive(Debug)]
pub enum Libp2pNetworkBackendEventKind {
    FullyMixedMessage,
}

#[derive(Debug, Clone)]
pub enum Libp2pNetworkBackendEvent {
    FullyMixedMessage(Vec<u8>),
}

const CHANNEL_SIZE: usize = 64;

#[async_trait]
impl NetworkBackend for Libp2pNetworkBackend {
    type Settings = Libp2pNetworkBackendSettings;
    type State = NoState<Self::Settings>;
    type Message = Libp2pNetworkBackendMessage;
    type EventKind = Libp2pNetworkBackendEventKind;
    type NetworkEvent = Libp2pNetworkBackendEvent;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let (msgs_tx, msgs_rx) = mpsc::channel(CHANNEL_SIZE);
        let (events_tx, _) = broadcast::channel(CHANNEL_SIZE);

        let keypair = Keypair::from(ed25519::Keypair::from(config.node_key.clone()));
        let local_peer_id = keypair.public().to_peer_id();
        let mut swarm = MixSwarm::new(keypair, config.num_mix_layers, msgs_rx, events_tx.clone());

        swarm
            .listen_on(config.listening_address)
            .unwrap_or_else(|e| {
                panic!("Failed to listen on Mix network: {e:?}");
            });

        // Randomly select peering_degree number of peers, and dial to them
        config
            .membership
            .iter()
            .filter(|addr| match extract_peer_id(addr) {
                Some(peer_id) => peer_id != local_peer_id,
                None => false,
            })
            .choose_multiple(&mut rand::thread_rng(), config.peering_degree)
            .iter()
            .cloned()
            .for_each(|addr| {
                if let Err(e) = swarm.dial(addr.clone()) {
                    tracing::error!("failed to dial to {:?}: {:?}", addr, e);
                }
            });

        let task = overwatch_handle.runtime().spawn(async move {
            swarm.run().await;
        });

        Self {
            task,
            msgs_tx,
            events_tx,
        }
    }

    async fn process(&self, msg: Self::Message) {
        if let Err(e) = self.msgs_tx.send(msg).await {
            tracing::error!("Failed to send message to MixSwarm: {e}");
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        match kind {
            Libp2pNetworkBackendEventKind::FullyMixedMessage => self.events_tx.subscribe(),
        }
    }
}

struct MixSwarm {
    swarm: Swarm<nomos_mix_network::Behaviour>,
    num_mix_layers: usize,
    msgs_rx: mpsc::Receiver<Libp2pNetworkBackendMessage>,
    events_tx: broadcast::Sender<Libp2pNetworkBackendEvent>,
}

impl MixSwarm {
    fn new(
        keypair: Keypair,
        num_mix_layers: usize,
        msgs_rx: mpsc::Receiver<Libp2pNetworkBackendMessage>,
        events_tx: broadcast::Sender<Libp2pNetworkBackendEvent>,
    ) -> Self {
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| {
                nomos_mix_network::Behaviour::new(nomos_mix_network::Config {
                    transmission_rate: 1.0,
                    duplicate_cache_lifespan: 60,
                })
            })
            .expect("Mix Behaviour should be built")
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
            })
            .build();

        Self {
            swarm,
            num_mix_layers,
            msgs_rx,
            events_tx,
        }
    }

    fn listen_on(&mut self, addr: Multiaddr) -> Result<ListenerId, TransportError<io::Error>> {
        self.swarm.listen_on(addr)
    }

    fn dial(&mut self, addr: Multiaddr) -> Result<(), DialError> {
        self.swarm.dial(DialOpts::from(addr))
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.msgs_rx.recv() => {
                    self.handle_msg(msg).await;
                }
                Some(event) = self.swarm.next() => {
                    self.handle_event(event);
                }
            }
        }
    }

    async fn handle_msg(&mut self, msg: Libp2pNetworkBackendMessage) {
        match msg {
            Libp2pNetworkBackendMessage::Mix(msg) => {
                tracing::debug!("Wrap msg and send it to mix network: {msg:?}");
                match nomos_mix_message::new_message(&msg, self.num_mix_layers.try_into().unwrap())
                {
                    Ok(wrapped_msg) => {
                        if let Err(e) = self.swarm.behaviour_mut().publish(wrapped_msg) {
                            tracing::error!("Failed to publish message to mix network: {e:?}");
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to wrap message: {e:?}");
                    }
                }
            }
        }
    }

    fn handle_event(&mut self, event: SwarmEvent<nomos_mix_network::Event>) {
        match event {
            SwarmEvent::Behaviour(nomos_mix_network::Event::FullyUnwrappedMessage(msg)) => {
                tracing::debug!("Received fully unwrapped message: {msg:?}");
                self.events_tx
                    .send(Libp2pNetworkBackendEvent::FullyMixedMessage(msg))
                    .unwrap();
            }
            SwarmEvent::Behaviour(nomos_mix_network::Event::Error(e)) => {
                tracing::error!("Received error from mix network: {e:?}");
            }
            _ => {
                tracing::debug!("Received event from mix network: {event:?}");
            }
        }
    }
}

fn extract_peer_id(multiaddr: &Multiaddr) -> Option<PeerId> {
    for protocol in multiaddr.iter() {
        if let Protocol::P2p(peer_id) = protocol {
            return Some(peer_id);
        }
    }
    None
}

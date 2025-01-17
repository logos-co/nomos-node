use std::{collections::HashSet, pin::Pin, time::Duration};

use super::BlendBackend;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use libp2p::{
    identity::{ed25519, Keypair},
    swarm::SwarmEvent,
    Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use nomos_blend::{conn_monitor::ConnectionMaintenanceSettings, membership::Membership};
use nomos_blend_message::sphinx::SphinxMessage;
use nomos_libp2p::secret_key_serde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

/// A blend backend that uses the libp2p network stack.
pub struct Libp2pBlendBackend {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    swarm_message_sender: mpsc::Sender<BlendSwarmMessage>,
    incoming_message_sender: broadcast::Sender<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Libp2pBlendBackendSettings {
    pub listening_address: Multiaddr,
    // A key for deriving PeerId and establishing secure connections (TLS 1.3 by QUIC)
    #[serde(with = "secret_key_serde", default = "ed25519::SecretKey::generate")]
    pub node_key: ed25519::SecretKey,
    pub conn_maintenance: ConnectionMaintenanceSettings,
}

const CHANNEL_SIZE: usize = 64;

#[async_trait]
impl BlendBackend for Libp2pBlendBackend {
    type Settings = Libp2pBlendBackendSettings;

    fn new<R>(
        config: Self::Settings,
        overwatch_handle: OverwatchHandle,
        membership: Membership<SphinxMessage>,
        rng: R,
    ) -> Self
    where
        R: RngCore + Send + 'static,
    {
        let (swarm_message_sender, swarm_message_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (incoming_message_sender, _) = broadcast::channel(CHANNEL_SIZE);

        let mut swarm = BlendSwarm::new(
            config,
            membership,
            rng,
            swarm_message_receiver,
            incoming_message_sender.clone(),
        );

        let task = overwatch_handle.runtime().spawn(async move {
            swarm.run().await;
        });

        Self {
            task,
            swarm_message_sender,
            incoming_message_sender,
        }
    }

    async fn publish(&self, msg: Vec<u8>) {
        if let Err(e) = self
            .swarm_message_sender
            .send(BlendSwarmMessage::Publish(msg))
            .await
        {
            tracing::error!("Failed to send message to BlendSwarm: {e}");
        }
    }

    fn listen_to_incoming_messages(&mut self) -> Pin<Box<dyn Stream<Item = Vec<u8>> + Send>> {
        Box::pin(
            BroadcastStream::new(self.incoming_message_sender.subscribe())
                .filter_map(|event| async { event.ok() }),
        )
    }
}

struct BlendSwarm<R>
where
    R: RngCore + 'static,
{
    swarm: Swarm<nomos_blend_network::Behaviour<SphinxMessage, R, IntervalStream>>,
    swarm_messages_receiver: mpsc::Receiver<BlendSwarmMessage>,
    incoming_message_sender: broadcast::Sender<Vec<u8>>,

    peering_degree: usize,
    max_peering_degree: usize,
    membership: Membership<SphinxMessage>,
    rng: R,
    blacklist_peers: HashSet<PeerId>,
}

#[derive(Debug)]
pub enum BlendSwarmMessage {
    Publish(Vec<u8>),
}

impl<R> BlendSwarm<R>
where
    R: RngCore + 'static,
{
    fn new(
        config: Libp2pBlendBackendSettings,
        membership: Membership<SphinxMessage>,
        rng: R,
        swarm_messages_receiver: mpsc::Receiver<BlendSwarmMessage>,
        incoming_message_sender: broadcast::Sender<Vec<u8>>,
    ) -> Self {
        let keypair = Keypair::from(ed25519::Keypair::from(config.node_key.clone()));
        let mut swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_quic()
            .with_behaviour(|_| {
                let conn_maintenance_interval =
                    config.conn_maintenance.monitor.as_ref().map(|monitor| {
                        IntervalStream::new(tokio::time::interval(monitor.time_window))
                    });
                nomos_blend_network::Behaviour::new(
                    nomos_blend_network::Config {
                        duplicate_cache_lifespan: 60,
                        conn_maintenance_settings: config.conn_maintenance,
                        conn_maintenance_interval,
                    },
                    membership,
                    rng,
                )
            })
            .expect("Blend Behaviour should be built")
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
            })
            .build();

        swarm
            .listen_on(config.listening_address)
            .unwrap_or_else(|e| {
                panic!("Failed to listen on Blend network: {e:?}");
            });

        Self {
            swarm,
            swarm_messages_receiver,
            incoming_message_sender,
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(msg) = self.swarm_messages_receiver.recv() => {
                    self.handle_swarm_message(msg).await;
                }
                Some(event) = self.swarm.next() => {
                    self.handle_event(event);
                }
            }
        }
    }

    async fn handle_swarm_message(&mut self, msg: BlendSwarmMessage) {
        match msg {
            BlendSwarmMessage::Publish(msg) => {
                let msg_size = msg.len();
                if let Err(e) = self.swarm.behaviour_mut().publish(msg) {
                    tracing::error!("Failed to publish message to blend network: {e:?}");
                    tracing::info!(counter.failed_outbound_messages = 1);
                } else {
                    tracing::info!(counter.successful_outbound_messages = 1);
                    tracing::info!(histogram.sent_data = msg_size as u64);
                }
            }
        }
    }

    fn handle_event(&mut self, event: SwarmEvent<nomos_blend_network::Event>) {
        match event {
            SwarmEvent::Behaviour(nomos_blend_network::Event::Message(msg)) => {
                tracing::debug!("Received message from a peer: {msg:?}");

                let msg_size = msg.len();
                if let Err(e) = self.incoming_message_sender.send(msg) {
                    tracing::error!("Failed to send incoming message to channel: {e}");
                    tracing::info!(counter.failed_inbound_messages = 1);
                } else {
                    tracing::info!(counter.successful_inbound_messages = 1);
                    tracing::info!(histogram.received_data = msg_size as u64);
                }
            }
            SwarmEvent::Behaviour(nomos_blend_network::Event::MaliciousPeer(peer_id)) => {
                tracing::debug!("A malicious peer detected: {peer_id}");
                self.blacklist_peers.insert(peer_id);
                if let Err(e) = self.select_and_dial_peer() {
                    tracing::error!("Failed to select and dial a new peer: {e}");
                }
            }
            SwarmEvent::Behaviour(nomos_blend_network::Event::UnhealthyPeer(peer_id)) => {
                tracing::debug!("A unhealthy peer detected: {peer_id}");
                if let Err(e) = self.select_and_dial_peer() {
                    tracing::error!("Failed to select and dial a new peer: {e}");
                }
            }
            SwarmEvent::Behaviour(nomos_blend_network::Event::Error(e)) => {
                tracing::error!("Received error from blend network: {e:?}");
                tracing::info!(counter.error = 1);
                //TODO: maybe dial a new peer?
            }
            _ => {
                tracing::debug!("Received event from blend network: {event:?}");
                tracing::info!(counter.ignored_event = 1);
            }
        }
    }

    fn select_and_dial_peer(&mut self) -> Result<Option<PeerId>, Error> {
        let negotiated_peers = self.swarm.behaviour().negotiated_peers();
        if negotiated_peers.len() >= self.max_peering_degree {
            return Ok(None);
        }

        let peers_to_exclude = self
            .blacklist_peers
            .union(self.swarm.behaviour().negotiated_peers())
            .cloned()
            .collect();
        if let Some(new_peer) = self
            .membership
            .filter_and_choose_remote_nodes(&mut self.rng, 1, &peers_to_exclude)
            .first()
        {
            self.swarm.dial(new_peer)?;
            Ok(Some(new_peer))
        } else {
            Ok(None)
        }
    }
}

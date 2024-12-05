use std::{pin::Pin, time::Duration};

use super::MixBackend;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use libp2p::{
    identity::{ed25519, Keypair},
    swarm::SwarmEvent,
    Multiaddr, Swarm, SwarmBuilder,
};
use nomos_libp2p::secret_key_serde;
use nomos_mix::{conn_maintenance::ConnectionMaintenanceSettings, membership::Membership};
use nomos_mix_message::sphinx::SphinxMessage;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};

/// A mix backend that uses the libp2p network stack.
pub struct Libp2pMixBackend {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    swarm_message_sender: mpsc::Sender<MixSwarmMessage>,
    incoming_message_sender: broadcast::Sender<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Libp2pMixBackendSettings {
    pub listening_address: Multiaddr,
    // A key for deriving PeerId and establishing secure connections (TLS 1.3 by QUIC)
    #[serde(with = "secret_key_serde", default = "ed25519::SecretKey::generate")]
    pub node_key: ed25519::SecretKey,
    pub conn_maintenance: ConnectionMaintenanceSettings,
}

const CHANNEL_SIZE: usize = 64;

#[async_trait]
impl MixBackend for Libp2pMixBackend {
    type Settings = Libp2pMixBackendSettings;
    type Address = Multiaddr;

    fn new<R>(
        config: Self::Settings,
        overwatch_handle: OverwatchHandle,
        membership: Membership<Self::Address, SphinxMessage>,
        rng: R,
    ) -> Self
    where
        R: RngCore + Send + 'static,
    {
        let (swarm_message_sender, swarm_message_receiver) = mpsc::channel(CHANNEL_SIZE);
        let (incoming_message_sender, _) = broadcast::channel(CHANNEL_SIZE);

        let mut swarm = MixSwarm::new(
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
            .send(MixSwarmMessage::Publish(msg))
            .await
        {
            tracing::error!("Failed to send message to MixSwarm: {e}");
        }
    }

    fn listen_to_incoming_messages(&mut self) -> Pin<Box<dyn Stream<Item = Vec<u8>> + Send>> {
        Box::pin(
            BroadcastStream::new(self.incoming_message_sender.subscribe())
                .filter_map(|event| async { event.ok() }),
        )
    }
}

struct MixSwarm<R>
where
    R: RngCore + 'static,
{
    swarm: Swarm<nomos_mix_network::Behaviour<SphinxMessage, R, IntervalStream>>,
    swarm_messages_receiver: mpsc::Receiver<MixSwarmMessage>,
    incoming_message_sender: broadcast::Sender<Vec<u8>>,
}

#[derive(Debug)]
pub enum MixSwarmMessage {
    Publish(Vec<u8>),
}

impl<R> MixSwarm<R>
where
    R: RngCore + 'static,
{
    fn new(
        config: Libp2pMixBackendSettings,
        membership: Membership<Multiaddr, SphinxMessage>,
        rng: R,
        swarm_messages_receiver: mpsc::Receiver<MixSwarmMessage>,
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
                nomos_mix_network::Behaviour::new(
                    nomos_mix_network::Config {
                        duplicate_cache_lifespan: 60,
                        conn_maintenance_settings: config.conn_maintenance,
                        conn_maintenance_interval,
                    },
                    membership,
                    rng,
                )
            })
            .expect("Mix Behaviour should be built")
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
            })
            .build();

        swarm
            .listen_on(config.listening_address)
            .unwrap_or_else(|e| {
                panic!("Failed to listen on Mix network: {e:?}");
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

    async fn handle_swarm_message(&mut self, msg: MixSwarmMessage) {
        match msg {
            MixSwarmMessage::Publish(msg) => {
                if let Err(e) = self.swarm.behaviour_mut().publish(msg) {
                    tracing::error!("Failed to publish message to mix network: {e:?}");
                }
            }
        }
    }

    fn handle_event(&mut self, event: SwarmEvent<nomos_mix_network::Event>) {
        match event {
            SwarmEvent::Behaviour(nomos_mix_network::Event::Message(msg)) => {
                tracing::debug!("Received message from a peer: {msg:?}");
                if let Err(e) = self.incoming_message_sender.send(msg) {
                    tracing::error!("Failed to send incoming message to channel: {e}");
                }
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

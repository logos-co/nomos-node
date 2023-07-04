pub mod command;
pub mod event;
mod swarm;

use std::error::Error;

use command::Command;
use event::Event;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use swarm::{Swarm, SwarmConfig};
use tokio::sync::{broadcast, mpsc, oneshot};

pub struct NomosLibp2p {
    pub peer_id: PeerId,
    command_tx: mpsc::Sender<Command>,
    event_tx: broadcast::Sender<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NomosLibp2pConfig {
    pub swarm_config: SwarmConfig,
    pub command_channel_size: usize,
    pub event_channel_size: usize,
}

impl Default for NomosLibp2pConfig {
    fn default() -> Self {
        Self {
            swarm_config: Default::default(),
            command_channel_size: 16,
            event_channel_size: 16,
        }
    }
}

impl NomosLibp2p {
    // TODO: define error types
    pub fn new(
        config: NomosLibp2pConfig,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<Self, Box<dyn Error>> {
        let (command_tx, command_rx) = mpsc::channel::<Command>(config.command_channel_size);
        let (event_tx, _) = broadcast::channel::<Event>(config.event_channel_size);

        let peer_id = Swarm::run(
            &config.swarm_config,
            runtime_handle,
            command_rx,
            event_tx.clone(),
        )?;

        Ok(Self {
            peer_id,
            command_tx,
            event_tx,
        })
    }

    // Create a receiver that will receive events emitted from libp2p swarm
    pub fn event_receiver(&self) -> broadcast::Receiver<Event> {
        self.event_tx.subscribe()
    }

    pub async fn send_command(
        &self,
        message: command::CommandMessage,
    ) -> Result<(), Box<dyn Error>> {
        let (sender, receiver) = oneshot::channel();
        self.command_tx
            .clone()
            .send(Command { message, sender })
            .await?;

        receiver.await?.map_err(|e| -> Box<dyn Error> { e })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::command::CommandMessage;

    use super::*;

    #[tokio::test]
    async fn subscribe() {
        env_logger::init();

        let config1 = NomosLibp2pConfig {
            swarm_config: SwarmConfig {
                port: 60000,
                ..Default::default()
            },
            ..Default::default()
        };
        let node1 = NomosLibp2p::new(config1, tokio::runtime::Handle::current()).unwrap();

        let config2 = NomosLibp2pConfig {
            swarm_config: SwarmConfig {
                port: 60001,
                ..Default::default()
            },
            ..Default::default()
        };
        let node2 = NomosLibp2p::new(config2, tokio::runtime::Handle::current()).unwrap();

        assert!(node2
            .send_command(CommandMessage::Connect(
                node1.peer_id,
                "/ip4/127.0.0.1/tcp/60000".parse().unwrap()
            ))
            .await
            .is_ok());

        let topic = "topic1".to_string();
        assert!(node2
            .send_command(CommandMessage::Subscribe(topic.clone()))
            .await
            .is_ok());

        // Wait until the node2's subscription is propagated to the node1.
        tokio::time::sleep(Duration::from_secs(3)).await;

        let message = "hello".as_bytes().to_vec();
        assert!(node1
            .send_command(CommandMessage::Broadcast {
                topic: topic.clone(),
                message: message.clone()
            })
            .await
            .is_ok());

        match node2.event_receiver().recv().await.unwrap() {
            Event::Message(msg) => {
                assert_eq!(msg.topic, libp2p::gossipsub::IdentTopic::new(topic).hash());
                assert_eq!(msg.data, message);
            }
        }
    }
}

use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    error::Error,
    hash::{Hash, Hasher},
};

use futures::StreamExt;
use libp2p::{
    core::upgrade,
    gossipsub,
    identity::{self, secp256k1},
    noise,
    swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent},
    tcp, yamux, PeerId, Transport,
};
use multiaddr::{multiaddr, Protocol};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast::Sender, mpsc::Receiver};

use crate::{
    command::{Command, CommandMessage, CommandSender},
    event::Event,
};

pub struct Swarm {
    swarm: libp2p::Swarm<Behaviour>,
    event_tx: Sender<Event>,
    pending_dial: HashMap<PeerId, CommandSender>,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmConfig {
    // Listening IPv4 address
    pub host: std::net::Ipv4Addr,
    // TCP listening port. Use 0 for random
    pub port: u16,
    /// Secp256k1 private key in Hex format (`0x123...abc`). Default random
    #[serde(with = "secret_key_serde")]
    pub node_key: secp256k1::SecretKey,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            host: std::net::Ipv4Addr::new(0, 0, 0, 0),
            port: 60000,
            node_key: secp256k1::SecretKey::generate(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SwarmError {
    #[error("duplicate dialing")]
    DuplicateDialing,
}

impl Swarm {
    // TODO: define error types
    pub fn run(
        config: &SwarmConfig,
        runtime_handle: tokio::runtime::Handle,
        mut command_rx: Receiver<Command>,
        event_tx: Sender<Event>,
    ) -> Result<PeerId, Box<dyn Error>> {
        let id_keys = identity::Keypair::from(secp256k1::Keypair::from(config.node_key.clone()));
        let local_peer_id = PeerId::from(id_keys.public());
        log::info!("libp2p peer_id:{}", local_peer_id);

        let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1Lazy)
            .authenticate(noise::Config::new(&id_keys)?)
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
                .build()?,
        )?;

        let mut swarm = SwarmBuilder::with_tokio_executor(
            tcp_transport,
            Behaviour { gossipsub },
            local_peer_id,
        )
        .build();

        swarm.listen_on(multiaddr!(Ip4(config.host), Tcp(config.port)))?;

        let mut swarm = Swarm {
            swarm,
            event_tx,
            pending_dial: Default::default(),
        };
        runtime_handle.spawn(async move {
            loop {
                tokio::select! {
                    event = swarm.swarm.select_next_some() => {
                        swarm.handle_swarm_event(event).await;
                    }
                    Some(command) = command_rx.recv() => {
                        swarm.handle_command(command).await;
                    }
                }
            }
        });

        Ok(local_peer_id)
    }

    async fn handle_swarm_event<T>(&mut self, swarm_event: SwarmEvent<BehaviourEvent, T>) {
        match swarm_event {
            SwarmEvent::NewListenAddr { address, .. } => {
                log::info!("libp2p local peer is listening on {address}");
            }
            SwarmEvent::ConnectionEstablished {
                peer_id, endpoint, ..
            } => {
                if endpoint.is_dialer() {
                    if let Some(sender) = self.pending_dial.remove(&peer_id) {
                        let _ = sender.send(Ok(()));
                    }
                }
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id: Some(peer_id),
                error,
                ..
            } => {
                if let Some(sender) = self.pending_dial.remove(&peer_id) {
                    let _ = sender.send(Err(Box::new(error)));
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: peer_id,
                message_id: id,
                message,
            })) => {
                log::debug!("Got message with id: {id} from peer: {peer_id}");
                self.emit_event(Event::Message(message));
            }
            _ => {
                //TODO: handle other events
            }
        }
    }

    fn emit_event(&mut self, event: Event) {
        if let Err(e) = self.event_tx.send(event) {
            log::error!("failed to emit event from libp2p swarm: {e}");
        }
    }

    async fn handle_command(&mut self, command: Command) {
        let Command { message, sender } = command;

        match message {
            CommandMessage::Connect(peer_id, peer_addr) => {
                if let std::collections::hash_map::Entry::Vacant(e) =
                    self.pending_dial.entry(peer_id)
                {
                    match self.swarm.dial(peer_addr.with(Protocol::P2p(peer_id))) {
                        Ok(_) => {
                            e.insert(sender);
                        }
                        Err(e) => {
                            let _ = sender.send(Err(Box::new(e)));
                        }
                    }
                } else {
                    let _ = sender.send(Err(Box::new(SwarmError::DuplicateDialing)));
                }
            }
            CommandMessage::Broadcast { topic, message } => {
                let result = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gossipsub::IdentTopic::new(topic), message);

                match result {
                    Ok(message_id) => {
                        log::debug!("message broadcasted: {message_id}");
                        let _ = sender.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = sender.send(Err(Box::new(e)));
                    }
                }
            }
            CommandMessage::Subscribe(topic) => {
                let result = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .subscribe(&gossipsub::IdentTopic::new(topic));

                match result {
                    Ok(_) => {
                        let _ = sender.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = sender.send(Err(Box::new(e)));
                    }
                }
            }
            CommandMessage::Unsubscribe(topic) => {
                let result = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .unsubscribe(&gossipsub::IdentTopic::new(topic));

                match result {
                    Ok(_) => {
                        let _ = sender.send(Ok(()));
                    }
                    Err(e) => {
                        let _ = sender.send(Err(Box::new(e)));
                    }
                }
            }
        }
    }
}

mod secret_key_serde {
    use libp2p::identity::secp256k1;
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(key: &secp256k1::SecretKey, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_str = hex::encode(key.to_bytes());
        hex_str.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<secp256k1::SecretKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = String::deserialize(deserializer)?;
        let mut key_bytes = hex::decode(hex_str).map_err(|e| D::Error::custom(format!("{e}")))?;
        secp256k1::SecretKey::try_from_bytes(key_bytes.as_mut_slice())
            .map_err(|e| D::Error::custom(format!("{e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_serde() {
        let config: SwarmConfig = Default::default();

        let serialized = serde_json::to_string(&config).unwrap();
        println!("{serialized}");

        let deserialized: SwarmConfig = serde_json::from_str(serialized.as_str()).unwrap();
        assert_eq!(deserialized.host, config.host);
        assert_eq!(deserialized.port, config.port);
        assert_eq!(deserialized.node_key.to_bytes(), config.node_key.to_bytes());
    }
}

mod command;
mod config;
#[cfg(feature = "mixnet")]
pub mod mixnet;
pub(crate) mod swarm;

// std
pub use self::command::{Command, Dial, Libp2pInfo, Topic};
pub use self::config::Libp2pConfig;
use self::swarm::SwarmHandler;

// internal
use super::NetworkBackend;
#[cfg(feature = "mixnet")]
use crate::backends::libp2p::mixnet::{init_mixnet, MixnetMessage, STREAM_PROTOCOL};
#[cfg(feature = "mixnet")]
use ::mixnet::client::MessageQueue;
pub use nomos_libp2p::libp2p::gossipsub::{Message, TopicHash};
// crates
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use tokio::sync::{broadcast, mpsc};

pub struct Libp2p {
    events_tx: broadcast::Sender<Event>,
    commands_tx: mpsc::Sender<Command>,
    #[cfg(feature = "mixnet")]
    mixclient_message_queue: MessageQueue,
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

/// Events emitted from [`NomosLibp2p`], which users can subscribe
#[derive(Debug, Clone)]
pub enum Event {
    Message(Message),
}

const BUFFER_SIZE: usize = 64;

#[async_trait::async_trait]
impl NetworkBackend for Libp2p {
    type Settings = Libp2pConfig;
    type State = NoState<Libp2pConfig>;
    type Message = Command;
    type EventKind = EventKind;
    type NetworkEvent = Event;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let (commands_tx, commands_rx) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);

        let mut swarm_handler =
            SwarmHandler::new(&config, commands_tx.clone(), commands_rx, events_tx.clone());

        #[cfg(feature = "mixnet")]
        let mixclient_message_queue = init_mixnet(
            config.mixnet,
            overwatch_handle.runtime().clone(),
            commands_tx.clone(),
            swarm_handler.incoming_streams(STREAM_PROTOCOL),
        );

        overwatch_handle.runtime().spawn(async move {
            swarm_handler.run(config.initial_peers).await;
        });

        Self {
            events_tx,
            commands_tx,
            #[cfg(feature = "mixnet")]
            mixclient_message_queue,
        }
    }

    #[cfg(not(feature = "mixnet"))]
    async fn process(&self, msg: Self::Message) {
        if let Err(e) = self.commands_tx.send(msg).await {
            tracing::error!("failed to send command to nomos-libp2p: {e:?}");
        }
    }

    #[cfg(feature = "mixnet")]
    async fn process(&self, msg: Self::Message) {
        match msg {
            Command::Broadcast { topic, message } => {
                let msg = MixnetMessage { topic, message };
                if let Err(e) = self.mixclient_message_queue.send(msg.as_bytes()).await {
                    tracing::error!("failed to send messasge to mixclient: {e}");
                }
            }
            cmd => {
                if let Err(e) = self.commands_tx.send(cmd).await {
                    tracing::error!("failed to send command to libp2p swarm: {e:?}");
                }
            }
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Message => {
                tracing::debug!("processed subscription to incoming messages");
                self.events_tx.subscribe()
            }
        }
    }
}

use nomos_libp2p::{command::CommandMessage, event::Event, NomosLibp2p, NomosLibp2pConfig};
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use tokio::sync::broadcast;

use super::NetworkBackend;

pub struct Libp2p {
    libp2p: NomosLibp2p,
}

#[derive(Debug)]
pub enum EventKind {
    Message,
}

#[async_trait::async_trait]
impl NetworkBackend for Libp2p {
    type Settings = NomosLibp2pConfig;
    type State = NoState<NomosLibp2pConfig>;
    type Message = CommandMessage;
    type EventKind = EventKind;
    type NetworkEvent = Event;

    fn new(config: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        let libp2p = NomosLibp2p::new(config, overwatch_handle.runtime().clone()).unwrap();
        Self { libp2p }
    }

    async fn process(&self, msg: Self::Message) {
        if let Err(e) = self.libp2p.send_command(msg).await {
            tracing::error!("failed to send command to nomos-libp2p: {e:?}");
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> broadcast::Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Message => {
                tracing::debug!("processed subscription to incoming messages");
                self.libp2p.event_receiver()
            }
        }
    }
}

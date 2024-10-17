use std::pin::Pin;

use futures::{Stream, StreamExt};
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::backends::NetworkBackend;

#[derive(Clone)]
pub struct Mock {
    events_tx: broadcast::Sender<MockEvent>,
}

#[derive(Debug)]
pub enum MockMessage {
    Mix(Vec<u8>),
}

#[derive(Debug)]
pub enum MockEventKind {
    FullyMixedMessage,
}

#[derive(Debug, Clone)]
pub enum MockEvent {
    FullyMixedMessage(Vec<u8>),
}

const CHANNEL_SIZE: usize = 16;

#[async_trait::async_trait]
impl NetworkBackend for Mock {
    type Settings = ();
    type State = NoState<Self::Settings>;
    type Message = MockMessage;
    type EventKind = MockEventKind;
    type NetworkEvent = MockEvent;

    fn new(_: Self::Settings, _: OverwatchHandle) -> Self {
        let (events_tx, _) = broadcast::channel(CHANNEL_SIZE);
        Self { events_tx }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            MockMessage::Mix(msg) => {
                // Do nothing with the msg. Just return it as it is through the event channel.
                self.events_tx
                    .send(MockEvent::FullyMixedMessage(msg))
                    .unwrap();
            }
        }
    }

    async fn subscribe(
        &mut self,
        kind: Self::EventKind,
    ) -> Pin<Box<dyn Stream<Item = Self::NetworkEvent> + Send>> {
        match kind {
            MockEventKind::FullyMixedMessage => Box::pin(
                BroadcastStream::new(self.events_tx.subscribe())
                    .filter_map(|event| async { event.ok() }),
            ),
        }
    }
}

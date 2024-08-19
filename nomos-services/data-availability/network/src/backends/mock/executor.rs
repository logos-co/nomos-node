use kzgrs_backend::common::{blob::DaBlob, build_attestation_message};
use overwatch_rs::{overwatch::handle::OverwatchHandle, services::state::NoState};
use serde::{Deserialize, Serialize};
use tokio::sync::{
    broadcast::{self, Receiver},
    mpsc,
};

use crate::backends::NetworkBackend;

const BUFFER_SIZE: usize = 64;

#[derive(Debug)]
pub enum EventKind {
    Dispersal,
    Sample,
}

// A subset of dispersal protocol messages that will come over the wire.
// Imitates the message that will come from libp2p behaviour.
// Assuming that behaviour will wrap the nomos_da_message types.
#[derive(Debug, Clone)]
pub enum DisperseMessage {
    DispersalSuccess {
        blob_id: [u8; 32],
        subnetwork_id: u32,
    },
}

// A subset of sample protocol messages that will come over the wire.
// Imitates the message that will come from libp2p behaviour
// Assuming that behaviour will wrap the nomos_da_message types.
#[derive(Debug, Clone)]
pub enum SampleMessage {
    SampleSuccess {
        blob: Box<DaBlob>,
        subnetwork_id: u32,
    },
}

#[derive(Debug, Clone)]
pub enum Event {
    Disperse(DisperseMessage),
    Sample(SampleMessage),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MockConfig {}

#[derive(Debug)]
pub enum Command {
    Disperse { blob: DaBlob, subnetwork_id: u32 },
}

#[derive(Clone)]
pub struct MockExecutorBackend {
    _config: MockConfig,
    _commands_tx: mpsc::Sender<Command>,
    events_tx: broadcast::Sender<Event>,
}

#[async_trait::async_trait]
impl NetworkBackend for MockExecutorBackend {
    type Settings = MockConfig;
    type State = NoState<MockConfig>;
    type Message = Command;
    type EventKind = EventKind;
    type NetworkEvent = Event;

    fn new(_config: Self::Settings, _: OverwatchHandle) -> Self {
        let (_commands_tx, _) = tokio::sync::mpsc::channel(BUFFER_SIZE);
        let (events_tx, _) = tokio::sync::broadcast::channel(BUFFER_SIZE);
        Self {
            _config,
            _commands_tx,
            events_tx,
        }
    }

    async fn process(&self, msg: Self::Message) {
        match msg {
            Command::Disperse {
                blob,
                subnetwork_id,
            } => {
                let blob_id = build_attestation_message(
                    &blob.aggregated_column_commitment,
                    &blob.rows_commitments,
                );

                let success_message = DisperseMessage::DispersalSuccess {
                    blob_id,
                    subnetwork_id,
                };

                let _ = self.events_tx.send(Event::Disperse(success_message));
            }
        }
    }

    async fn subscribe(&mut self, kind: Self::EventKind) -> Receiver<Self::NetworkEvent> {
        match kind {
            EventKind::Dispersal | EventKind::Sample => self.events_tx.subscribe(),
        }
    }
}

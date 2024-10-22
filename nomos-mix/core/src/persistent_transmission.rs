use std::time::Duration;

use nomos_mix_message::DROP_MESSAGE;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::mpsc::{self, error::TryRecvError},
    time,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentTransmissionSettings {
    /// The maximum number of messages that can be emitted per second
    pub max_emission_frequency: f64,
    /// The probability of emitting a drop message by coin flipping
    pub drop_message_probability: f64,
}

impl Default for PersistentTransmissionSettings {
    fn default() -> Self {
        Self {
            max_emission_frequency: 1.0,
            drop_message_probability: 0.5,
        }
    }
}

/// Transmit scheduled messages with a persistent rate to the transmission channel.
///
/// # Arguments
///
/// * `settings` - The settings for the persistent transmission
/// * `schedule_receiver` - The channel for scheduled messages
/// * `emission_sender` - The channel to emit messages
pub async fn persistent_transmission(
    settings: PersistentTransmissionSettings,
    schedule_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    emission_sender: mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut schedule_receiver = schedule_receiver;
    let mut interval = time::interval(Duration::from_secs_f64(
        1.0 / settings.max_emission_frequency,
    ));

    loop {
        interval.tick().await;

        // Emit the first one of the scheduled messages.
        // If there is no scheduled message, emit a drop message with probability.
        match schedule_receiver.try_recv() {
            Ok(msg) => {
                if let Err(e) = emission_sender.send(msg) {
                    tracing::error!("Failed to send message to the transmission channel: {e:?}");
                }
            }
            Err(TryRecvError::Empty) => {
                // Flip a coin with drop_message_probability.
                // If the coin is head, emit the drop message.
                if coin_flip(settings.drop_message_probability) {
                    if let Err(e) = emission_sender.send(DROP_MESSAGE.to_vec()) {
                        tracing::error!(
                            "Failed to send drop message to the transmission channel: {e:?}"
                        );
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                tracing::error!("The schedule channel has been closed");
                break;
            }
        }
    }
}

fn coin_flip(probability: f64) -> bool {
    rand::thread_rng().gen::<f64>() < probability
}

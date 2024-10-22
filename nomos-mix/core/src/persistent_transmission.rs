use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

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
    _settings: PersistentTransmissionSettings,
    mut schedule_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    emission_sender: mpsc::UnboundedSender<Vec<u8>>,
) {
    // TODO: This is a mock implementation.
    //   The actual implementation must use emission frequency and emits drop messages.
    while let Some(message) = schedule_receiver.recv().await {
        emission_sender.send(message).unwrap();
    }
}

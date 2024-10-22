mod crypto;
mod temporal;

pub use crypto::CryptographicProcessorSettings;
pub use temporal::TemporalProcessorSettings;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::processor::crypto::CryptographicProcessor;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProcessorSettings {
    pub cryptographic_processor: CryptographicProcessorSettings,
    pub temporal_processor: TemporalProcessorSettings,
}

/// [`Processor`] implements "Message Router" defined in the Tier-2 spec.
/// - Wraps new messages using [`CryptographicProcessor`]
/// - Unwraps incoming messages received from network using [`CryptographicProcessor`]
/// - Pushes unwrapped messages to [`TemporalProcessor`] to delay them
/// - Releases messages returned by [`TemporalProcessor`]
pub struct Processor {
    /// To receive new messages originated from this node
    new_message_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    /// To receive incoming messages from received the network
    inbound_message_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    /// To release messages that are successfully processed but still wrapped
    outbound_message_sender: mpsc::UnboundedSender<Vec<u8>>,
    /// To release fully unwrapped messages
    fully_unwrapped_message_sender: mpsc::UnboundedSender<Vec<u8>>,
    /// To wrap and unwrap messages
    cryptographic_processor: CryptographicProcessor,
    // TODO: Add TemporalProcessor
}

impl Processor {
    pub fn new(
        settings: ProcessorSettings,
        new_message_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        inbound_message_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
        outbound_message_sender: mpsc::UnboundedSender<Vec<u8>>,
        fully_unwrapped_message_sender: mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        Self {
            new_message_receiver,
            inbound_message_receiver,
            outbound_message_sender,
            fully_unwrapped_message_sender,
            cryptographic_processor: CryptographicProcessor::new(settings.cryptographic_processor),
            // TODO: Initialize TemporalProcessor
        }
    }

    pub async fn run(&mut self) {
        // TODO: This is a mock implmementation.
        //  The actual implementation must use TemporalProcessor.
        loop {
            tokio::select! {
                Some(new_message) = self.new_message_receiver.recv() => {
                    let msg = self.cryptographic_processor.wrap_message(&new_message).unwrap();
                    self.outbound_message_sender.send(msg).unwrap();
                }
                Some(incoming_message) = self.inbound_message_receiver.recv() => {
                    let (msg, fully_unwrapped) = self.cryptographic_processor.unwrap_message(&incoming_message).unwrap();
                    assert!(fully_unwrapped);
                    self.fully_unwrapped_message_sender.send(msg).unwrap();
                }
            }
        }
    }
}

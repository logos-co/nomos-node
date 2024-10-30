mod crypto;
pub mod persistent_transmission;
mod temporal;

pub use crypto::CryptographicProcessorSettings;
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
pub use temporal::TemporalProcessorSettings;

use crate::message_blend::crypto::CryptographicProcessor;
use crate::message_blend::persistent_transmission::{
    PersistentTransmissionExt, PersistentTransmissionSettings,
};
use crate::message_blend::temporal::TemporalProcessorExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageBlendSettings {
    pub cryptographic_processor: CryptographicProcessorSettings,
    pub temporal_processor: TemporalProcessorSettings,
    pub persistent_transmission: PersistentTransmissionSettings,
}

pub enum MessageBlendStreamIncomingMessage {
    Local(Vec<u8>),
    Inbound(Vec<u8>),
}

pub enum MessageBlendStreamOutgoingMessage {
    FullyUnwrapped(Vec<u8>),
    Outbound(Vec<u8>),
}

/// [`MessageBlend`] handles the entire Tier-2 spec.
/// - Wraps new messages using [`CryptographicProcessor`]
/// - Unwraps incoming messages received from network using [`CryptographicProcessor`]
/// - Pushes unwrapped messages to [`TemporalProcessor`]
/// - Releases messages returned by [`TemporalProcessor`] to the proper channel
pub struct MessageBlendStream<S> {
    input_stream: S,
    output_stream: BoxStream<'static, MessageBlendStreamOutgoingMessage>,
    bypass_sender: UnboundedSender<Vec<u8>>,
    temporal_sender: UnboundedSender<MessageBlendStreamOutgoingMessage>,
    cryptographic_processor: CryptographicProcessor,
}

impl<S> MessageBlendStream<S>
where
    S: Stream<Item = MessageBlendStreamIncomingMessage>,
{
    pub fn new(input_stream: S, settings: MessageBlendSettings) -> Self {
        let cryptographic_processor = CryptographicProcessor::new(settings.cryptographic_processor);
        let (bypass_sender, bypass_receiver) = mpsc::unbounded_channel();
        let (temporal_sender, temporal_receiver) = mpsc::unbounded_channel();
        let output_stream = tokio_stream::StreamExt::merge(
            UnboundedReceiverStream::new(bypass_receiver)
                .persistent_transmission(settings.persistent_transmission)
                .map(MessageBlendStreamOutgoingMessage::Outbound),
            UnboundedReceiverStream::new(temporal_receiver)
                .to_temporal_stream(settings.temporal_processor),
        )
        .boxed();
        Self {
            input_stream,
            output_stream,
            bypass_sender,
            temporal_sender,
            cryptographic_processor,
        }
    }

    fn process_new_message(self: &mut Pin<&mut Self>, message: Vec<u8>) {
        match self.cryptographic_processor.wrap_message(&message) {
            Ok(wrapped_message) => {
                if let Err(e) = self.bypass_sender.send(wrapped_message) {
                    tracing::error!("Failed to send message to the outbound channel: {e:?}");
                }
            }
            Err(e) => {
                tracing::error!("Failed to wrap message: {:?}", e);
            }
        }
    }

    fn process_incoming_message(self: &mut Pin<&mut Self>, message: Vec<u8>) {
        match self.cryptographic_processor.unwrap_message(&message) {
            Ok((unwrapped_message, fully_unwrapped)) => {
                let message = if fully_unwrapped {
                    MessageBlendStreamOutgoingMessage::FullyUnwrapped(unwrapped_message)
                } else {
                    MessageBlendStreamOutgoingMessage::Outbound(unwrapped_message)
                };
                if let Err(e) = self.temporal_sender.send(message) {
                    tracing::error!("Failed to send message to the outbound channel: {e:?}");
                }
            }
            Err(nomos_mix_message::Error::MsgUnwrapNotAllowed) => {
                tracing::debug!("Message cannot be unwrapped by this node");
            }
            Err(e) => {
                tracing::error!("Failed to unwrap message: {:?}", e);
            }
        }
    }
}

impl<S> Stream for MessageBlendStream<S>
where
    S: Stream<Item = MessageBlendStreamIncomingMessage> + Unpin,
{
    type Item = MessageBlendStreamOutgoingMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.input_stream.poll_next_unpin(cx) {
            Poll::Ready(Some(MessageBlendStreamIncomingMessage::Local(message))) => {
                self.process_new_message(message);
            }
            Poll::Ready(Some(MessageBlendStreamIncomingMessage::Inbound(message))) => {
                self.process_incoming_message(message);
            }
            _ => {}
        }
        self.output_stream.poll_next_unpin(cx)
    }
}

pub trait MessageBlendExt: Stream<Item = MessageBlendStreamIncomingMessage> {
    fn blend(self, message_blend_settings: MessageBlendSettings) -> MessageBlendStream<Self>
    where
        Self: Sized,
    {
        MessageBlendStream::new(self, message_blend_settings)
    }
}

impl<T> MessageBlendExt for T where T: Stream<Item = MessageBlendStreamIncomingMessage> {}

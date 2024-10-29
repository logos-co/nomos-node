mod crypto;
mod temporal;

pub use crypto::CryptographicProcessorSettings;
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
pub use temporal::TemporalProcessorSettings;

use crate::message_blend::temporal::TemporalProcessorExt;
use crate::message_blend::{crypto::CryptographicProcessor, temporal::TemporalProcessor};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageBlendSettings {
    pub cryptographic_processor: CryptographicProcessorSettings,
    pub temporal_processor: TemporalProcessorSettings,
}

/// [`MessageBlend`] handles the entire Tier-2 spec.
/// - Wraps new messages using [`CryptographicProcessor`]
/// - Unwraps incoming messages received from network using [`CryptographicProcessor`]
/// - Pushes unwrapped messages to [`TemporalProcessor`]
/// - Releases messages returned by [`TemporalProcessor`] to the proper channel
pub struct MessageBlend {
    /// To receive new messages originated from this node
    new_message_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    /// To receive incoming messages from the network
    inbound_message_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    /// To release messages that are successfully processed but still wrapped
    outbound_message_sender: mpsc::UnboundedSender<Vec<u8>>,
    /// To release fully unwrapped messages
    fully_unwrapped_message_sender: mpsc::UnboundedSender<Vec<u8>>,
    /// Processors
    cryptographic_processor: CryptographicProcessor,
    temporal_processor: TemporalProcessor<TemporalProcessableMessage>,
}

impl MessageBlend {
    pub fn new(
        settings: MessageBlendSettings,
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
            temporal_processor: TemporalProcessor::<_>::new(settings.temporal_processor),
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(new_message) = self.new_message_receiver.recv() => {
                    self.handle_new_message(new_message);
                }
                Some(incoming_message) = self.inbound_message_receiver.recv() => {
                    self.handle_incoming_message(incoming_message);
                }
                Some(msg) = self.temporal_processor.next() => {
                    self.release_temporal_processed_message(msg);
                }
            }
        }
    }

    fn handle_new_message(&mut self, message: Vec<u8>) {
        match self.cryptographic_processor.wrap_message(&message) {
            Ok(wrapped_message) => {
                // Bypass Temporal Processor, and send the message to the outbound channel directly
                // because the message is originated from this node.
                if let Err(e) = self.outbound_message_sender.send(wrapped_message) {
                    tracing::error!("Failed to send message to the outbound channel: {e:?}");
                }
            }
            Err(e) => {
                tracing::error!("Failed to wrap message: {:?}", e);
            }
        }
    }

    fn handle_incoming_message(&mut self, message: Vec<u8>) {
        match self.cryptographic_processor.unwrap_message(&message) {
            Ok((unwrapped_message, fully_unwrapped)) => {
                self.temporal_processor
                    .push_message(TemporalProcessableMessage {
                        message: unwrapped_message,
                        fully_unwrapped,
                    });
            }
            Err(nomos_mix_message::Error::MsgUnwrapNotAllowed) => {
                tracing::debug!("Message cannot be unwrapped by this node");
            }
            Err(e) => {
                tracing::error!("Failed to unwrap message: {:?}", e);
            }
        }
    }

    fn release_temporal_processed_message(&mut self, message: TemporalProcessableMessage) {
        if message.fully_unwrapped {
            if let Err(e) = self.fully_unwrapped_message_sender.send(message.message) {
                tracing::error!(
                    "Failed to send fully unwrapped message to the fully unwrapped channel: {e:?}"
                );
            }
        } else if let Err(e) = self.outbound_message_sender.send(message.message) {
            tracing::error!("Failed to send message to the outbound channel: {e:?}");
        }
    }
}

#[derive(Clone)]
struct TemporalProcessableMessage {
    message: Vec<u8>,
    fully_unwrapped: bool,
}

pub enum MessageBlendStreamIncomingMessage {
    Local(Vec<u8>),
    Inbound(Vec<u8>),
}

pub enum MessageBlendStreamOutgoingMessage {
    FullyUnwrapped(Vec<u8>),
    Outbound(Vec<u8>),
}

pub struct MessageBlendStream<S> {
    input_stream: S,
    output_stream: BoxStream<'static, MessageBlendStreamOutgoingMessage>,
    bypass_sender: UnboundedSender<MessageBlendStreamOutgoingMessage>,
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
            UnboundedReceiverStream::new(bypass_receiver),
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
                if let Err(e) = self
                    .bypass_sender
                    .send(MessageBlendStreamOutgoingMessage::Outbound(wrapped_message))
                {
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

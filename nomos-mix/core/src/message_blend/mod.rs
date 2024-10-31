pub mod crypto;
pub mod temporal;

pub use crypto::CryptographicProcessorSettings;
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
pub use temporal::TemporalProcessorSettings;

use crate::message_blend::crypto::CryptographicProcessor;
use crate::message_blend::temporal::TemporalProcessorExt;
use crate::MixOutgoingMessage;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageBlendSettings {
    pub cryptographic_processor: CryptographicProcessorSettings,
    pub temporal_processor: TemporalProcessorSettings,
}

/// [`MessageBlendStream`] handles the entire mixing tiers process
/// - Unwraps incoming messages received from network using [`CryptographicProcessor`]
/// - Pushes unwrapped messages to [`TemporalProcessor`]
pub struct MessageBlendStream<S> {
    input_stream: S,
    output_stream: BoxStream<'static, MixOutgoingMessage>,
    temporal_sender: UnboundedSender<MixOutgoingMessage>,
    cryptographic_processor: CryptographicProcessor,
}

impl<S> MessageBlendStream<S>
where
    S: Stream<Item = Vec<u8>>,
{
    pub fn new(input_stream: S, settings: MessageBlendSettings) -> Self {
        let cryptographic_processor = CryptographicProcessor::new(settings.cryptographic_processor);
        let (temporal_sender, temporal_receiver) = mpsc::unbounded_channel();
        let output_stream = UnboundedReceiverStream::new(temporal_receiver)
            .temporal_stream(settings.temporal_processor)
            .boxed();
        Self {
            input_stream,
            output_stream,
            temporal_sender,
            cryptographic_processor,
        }
    }

    // fn process_new_message(self: &mut Pin<&mut Self>, message: Vec<u8>) {
    //     match self.cryptographic_processor.wrap_message(&message) {
    //         Ok(wrapped_message) => {
    //             if let Err(e) = self
    //                 .bypass_sender
    //                 .send(MixOutgoingMessage::Outbound(wrapped_message))
    //             {
    //                 tracing::error!("Failed to send message to the outbound channel: {e:?}");
    //             }
    //         }
    //         Err(e) => {
    //             tracing::error!("Failed to wrap message: {:?}", e);
    //         }
    //     }
    // }

    fn process_incoming_message(self: &mut Pin<&mut Self>, message: Vec<u8>) {
        match self.cryptographic_processor.unwrap_message(&message) {
            Ok((unwrapped_message, fully_unwrapped)) => {
                let message = if fully_unwrapped {
                    MixOutgoingMessage::FullyUnwrapped(unwrapped_message)
                } else {
                    MixOutgoingMessage::Outbound(unwrapped_message)
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
    S: Stream<Item = Vec<u8>> + Unpin,
{
    type Item = MixOutgoingMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(message) = self.input_stream.poll_next_unpin(cx) {
            self.process_incoming_message(message);
        }
        self.output_stream.poll_next_unpin(cx)
    }
}

pub trait MessageBlendExt: Stream<Item = Vec<u8>> {
    fn blend(self, message_blend_settings: MessageBlendSettings) -> MessageBlendStream<Self>
    where
        Self: Sized,
    {
        MessageBlendStream::new(self, message_blend_settings)
    }
}

impl<T> MessageBlendExt for T where T: Stream<Item = Vec<u8>> {}

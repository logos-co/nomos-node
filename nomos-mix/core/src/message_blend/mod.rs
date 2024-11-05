pub mod crypto;
pub mod temporal;

pub use crypto::CryptographicProcessorSettings;
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use rand::Rng;
use std::pin::Pin;
use std::task::{Context, Poll};
pub use temporal::TemporalProcessorSettings;

use crate::membership::Membership;
use crate::message_blend::crypto::CryptographicProcessor;
use crate::message_blend::temporal::TemporalProcessorExt;
use crate::MixOutgoingMessage;
use nomos_mix_message::MixMessage;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageBlendSettings<M>
where
    M: MixMessage,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
{
    pub cryptographic_processor: CryptographicProcessorSettings<M::PrivateKey>,
    pub temporal_processor: TemporalProcessorSettings,
}

/// [`MessageBlendStream`] handles the entire mixing tiers process
/// - Unwraps incoming messages received from network using [`CryptographicProcessor`]
/// - Pushes unwrapped messages to [`TemporalProcessor`]
pub struct MessageBlendStream<S, R, M>
where
    M: MixMessage,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
{
    input_stream: S,
    output_stream: BoxStream<'static, MixOutgoingMessage>,
    temporal_sender: UnboundedSender<MixOutgoingMessage>,
    cryptographic_processor: CryptographicProcessor<R, M>,
}

impl<S, R, M> MessageBlendStream<S, R, M>
where
    S: Stream<Item = Vec<u8>>,
    R: Rng,
    M: MixMessage,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
{
    pub fn new(
        input_stream: S,
        settings: MessageBlendSettings<M>,
        membership: Membership<M>,
        rng: R,
    ) -> Self {
        let cryptographic_processor =
            CryptographicProcessor::new(settings.cryptographic_processor, membership, rng);
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

impl<S, R, M> Stream for MessageBlendStream<S, R, M>
where
    S: Stream<Item = Vec<u8>> + Unpin,
    R: Rng + Unpin,
    M: MixMessage + Unpin,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq + Unpin,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq + Unpin,
{
    type Item = MixOutgoingMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(message)) = self.input_stream.poll_next_unpin(cx) {
            self.process_incoming_message(message);
        }
        self.output_stream.poll_next_unpin(cx)
    }
}

pub trait MessageBlendExt<M>: Stream<Item = Vec<u8>>
where
    M: MixMessage,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
{
    fn blend<R>(
        self,
        message_blend_settings: MessageBlendSettings<M>,
        membership: Membership<M>,
        rng: R,
    ) -> MessageBlendStream<Self, R, M>
    where
        Self: Sized + Unpin,
        R: Rng,
    {
        MessageBlendStream::new(self, message_blend_settings, membership, rng)
    }
}

impl<T, M> MessageBlendExt<M> for T
where
    T: Stream<Item = Vec<u8>>,
    M: MixMessage,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
{
}

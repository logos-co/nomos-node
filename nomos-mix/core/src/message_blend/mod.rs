pub mod crypto;
pub mod temporal;

pub use crypto::CryptographicProcessorSettings;
use futures::{Stream, StreamExt};
use rand::RngCore;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
pub use temporal::TemporalSchedulerSettings;

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
    M::PrivateKey: Serialize + DeserializeOwned,
{
    pub cryptographic_processor: CryptographicProcessorSettings<M::PrivateKey>,
    pub temporal_processor: TemporalSchedulerSettings,
}

/// [`MessageBlendStream`] handles the entire mixing tiers process
/// - Unwraps incoming messages received from network using [`CryptographicProcessor`]
/// - Pushes unwrapped messages to [`TemporalProcessor`]
pub struct MessageBlendStream<S, Rng, M, Scheduler>
where
    M: MixMessage,
{
    input_stream: S,
    output_stream: Pin<Box<dyn Stream<Item = MixOutgoingMessage> + Send + Sync + 'static>>,
    temporal_sender: UnboundedSender<MixOutgoingMessage>,
    cryptographic_processor: CryptographicProcessor<Rng, M>,
    _rng: PhantomData<Rng>,
    _scheduler: PhantomData<Scheduler>,
}

impl<S, Rng, M, Scheduler> MessageBlendStream<S, Rng, M, Scheduler>
where
    S: Stream<Item = Vec<u8>>,
    Rng: RngCore + Unpin + Send + 'static,
    M: MixMessage,
    M::PrivateKey: Serialize + DeserializeOwned,
    M::PublicKey: Clone + PartialEq,
    Scheduler: Stream<Item = ()> + Unpin + Send + Sync + 'static,
{
    pub fn new(
        input_stream: S,
        settings: MessageBlendSettings<M>,
        membership: Membership<M>,
        scheduler: Scheduler,
        cryptographic_processor_rng: Rng,
    ) -> Self {
        let cryptographic_processor = CryptographicProcessor::new(
            settings.cryptographic_processor,
            membership,
            cryptographic_processor_rng,
        );
        let (temporal_sender, temporal_receiver) = mpsc::unbounded_channel();
        let output_stream =
            Box::pin(UnboundedReceiverStream::new(temporal_receiver).temporal_stream(scheduler));
        Self {
            input_stream,
            output_stream,
            temporal_sender,
            cryptographic_processor,
            _rng: Default::default(),
            _scheduler: Default::default(),
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

impl<S, Rng, M, Scheduler> Stream for MessageBlendStream<S, Rng, M, Scheduler>
where
    S: Stream<Item = Vec<u8>> + Unpin,
    Rng: RngCore + Unpin + Send + 'static,
    M: MixMessage + Unpin,
    M::PrivateKey: Serialize + DeserializeOwned + Unpin,
    M::PublicKey: Clone + PartialEq + Unpin,
    Scheduler: Stream<Item = ()> + Unpin + Send + Sync + 'static,
{
    type Item = MixOutgoingMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(message)) = self.input_stream.poll_next_unpin(cx) {
            self.process_incoming_message(message);
        }
        self.output_stream.poll_next_unpin(cx)
    }
}

pub trait MessageBlendExt<Rng, M, Scheduler>: Stream<Item = Vec<u8>>
where
    Rng: RngCore + Send + Unpin + 'static,
    M: MixMessage,
    M::PrivateKey: Serialize + DeserializeOwned,
    M::PublicKey: Clone + PartialEq,
    Scheduler: Stream<Item = ()> + Unpin + Send + Sync + 'static,
{
    fn blend(
        self,
        message_blend_settings: MessageBlendSettings<M>,
        membership: Membership<M>,
        scheduler: Scheduler,
        cryptographic_processor_rng: Rng,
    ) -> MessageBlendStream<Self, Rng, M, Scheduler>
    where
        Self: Sized + Unpin,
    {
        MessageBlendStream::new(
            self,
            message_blend_settings,
            membership,
            scheduler,
            cryptographic_processor_rng,
        )
    }
}

impl<T, Rng, M, S> MessageBlendExt<Rng, M, S> for T
where
    T: Stream<Item = Vec<u8>>,
    Rng: RngCore + Unpin + Send + 'static,
    M: MixMessage,
    M::PrivateKey: Clone + Serialize + DeserializeOwned + PartialEq,
    M::PublicKey: Clone + Serialize + DeserializeOwned + PartialEq,
    S: Stream<Item = ()> + Unpin + Send + Sync + 'static,
{
}

use std::{
    io::Error,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, prelude::*, Future};
use libp2p::{
    core::upgrade::ReadyUpgrade,
    swarm::{
        handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound},
        ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol,
    },
    Stream, StreamProtocol,
};
use log::trace;
use nomos_da_messages::packing::{pack_to_writer, unpack_from_reader};
use tracing::error;

use crate::protocol::REPLICATION_PROTOCOL;

pub type DaMessage = nomos_da_messages::replication::ReplicationRequest;

/// Events that bubbles up from the `BroadcastHandler` to the `NetworkBehaviour`
#[expect(
    clippy::large_enum_variant,
    reason = "TODO: Address this at some point."
)]
#[derive(Debug)]
pub enum HandlerEventToBehaviour {
    IncomingMessage { message: DaMessage },
    OutgoingMessageError { error: Error },
}

/// Events that bubbles up from the `NetworkBehaviour` to the `BroadcastHandler`
#[derive(Debug)]
pub enum BehaviourEventToHandler {
    OutgoingMessage { message: DaMessage },
}

/// Broadcast configuration
pub(crate) struct ReplicationHandlerConfig {}

impl ReplicationHandlerConfig {
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for ReplicationHandlerConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// State handling for outgoing broadcast messages
enum OutboundState {
    OpenStream,
    Idle(Stream),
    Sending(future::BoxFuture<'static, Result<Stream, Error>>),
}

/// Broadcasting handler for the broadcast protocol
/// Forwards and read messages
pub struct ReplicationHandler {
    // incoming messages stream
    inbound: Option<BoxFuture<'static, Result<(DaMessage, Stream), Error>>>,
    // outgoing messages stream
    outbound: Option<OutboundState>,
    // pending messages not propagated in the connection
    outgoing_messages: Vec<DaMessage>,
}

impl ReplicationHandler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inbound: None,
            outbound: None,
            outgoing_messages: Vec::default(),
        }
    }
}

impl Default for ReplicationHandler {
    fn default() -> Self {
        Self::new()
    }
}

fn read_message(mut stream: Stream) -> impl Future<Output = Result<(DaMessage, Stream), Error>> {
    trace!("Reading messages");
    async move {
        let unpacked_message = unpack_from_reader(&mut stream).await?;
        Ok((unpacked_message, stream))
    }
}

impl ReplicationHandler {
    fn send_pending_messages(
        &mut self,
        mut stream: Stream,
    ) -> impl Future<Output = Result<Stream, Error>> {
        trace!("Sending messages");
        let mut pending_messages = Vec::new();
        std::mem::swap(&mut self.outgoing_messages, &mut pending_messages);
        async {
            trace!("Writing {} messages", pending_messages.len());

            for message in pending_messages {
                pack_to_writer(&message, &mut stream)
                    .await
                    .unwrap_or_else(|_| {
                        panic!("Message should always be serializable.\nMessage: '{message:?}'")
                    });
                stream.flush().await?;
            }

            Ok(stream)
        }
    }

    fn poll_pending_incoming_messages(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Option<Result<DaMessage, Error>> {
        match self.inbound.take() {
            Some(mut future) => {
                let mut read = std::pin::pin!(&mut future);
                match read.poll_unpin(cx) {
                    Poll::Ready(Ok((message, stream))) => {
                        self.inbound = Some(read_message(stream).boxed());
                        Some(Ok(message))
                    }
                    Poll::Ready(Err(e)) => Some(Err(e)),
                    Poll::Pending => {
                        self.inbound = Some(future);
                        None
                    }
                }
            }
            _ => None,
        }
    }

    fn poll_pending_outgoing_messages(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Option<ConnectionHandlerEvent<ReadyUpgrade<StreamProtocol>, (), HandlerEventToBehaviour>>
    {
        // Propagate incoming messages
        match self.outbound.take() {
            Some(OutboundState::OpenStream) => {
                self.outbound = Some(OutboundState::OpenStream);
            }
            Some(OutboundState::Idle(stream)) if !self.outgoing_messages.is_empty() => {
                self.outbound = Some(OutboundState::Sending(
                    self.send_pending_messages(stream).boxed(),
                ));
            }
            Some(OutboundState::Idle(stream)) => {
                self.outbound = Some(OutboundState::Idle(stream));
            }
            Some(OutboundState::Sending(mut future)) => match future.poll_unpin(cx) {
                Poll::Ready(Ok(stream)) => {
                    self.outbound = Some(OutboundState::Idle(stream));
                }
                Poll::Ready(Err(e)) => {
                    error!("{e:?}");
                }
                Poll::Pending => {
                    self.outbound = Some(OutboundState::Sending(future));
                }
            },
            None => {
                self.outbound = Some(OutboundState::OpenStream);
                return Some(ConnectionHandlerEvent::OutboundSubstreamRequest {
                    protocol: <Self as ConnectionHandler>::listen_protocol(self),
                });
            }
        }
        None
    }
}
impl ConnectionHandler for ReplicationHandler {
    type FromBehaviour = BehaviourEventToHandler;
    type ToBehaviour = HandlerEventToBehaviour;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    #[expect(deprecated, reason = "Self::InboundProtocol is deprecated.")]
    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(ReadyUpgrade::new(REPLICATION_PROTOCOL), ())
    }

    #[expect(deprecated, reason = "Self::OutboundProtocol is deprecated.")]
    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if let Some(event) = self.poll_pending_outgoing_messages(cx) {
            // bubble up event
            return Poll::Ready(event);
        };
        match self.poll_pending_incoming_messages(cx) {
            Some(Ok(message)) => {
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                    HandlerEventToBehaviour::IncomingMessage { message },
                ));
            }
            Some(Err(e)) => {
                error!("Incoming DA message error {e}");
            }
            _ => {}
        }
        // poll again whenever the executor wants to
        cx.waker().wake_by_ref();
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            BehaviourEventToHandler::OutgoingMessage { message } => {
                trace!("Received outgoing message");
                self.outgoing_messages.push(message);
            }
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<Self::InboundProtocol, Self::OutboundProtocol>,
    ) {
        let is_outbound = event.is_outbound();
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                info: (),
            }) => {
                trace!("Received inbound stream");
                self.inbound = Some(read_message(stream).boxed());
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                info: (),
            }) => {
                trace!("Received outbound stream");
                self.outbound = Some(OutboundState::Idle(stream));
            }
            ConnectionEvent::DialUpgradeError(error) => {
                trace!("Upgrage error: {error:?}");
                if is_outbound {
                    self.outbound = None;
                }
            }
            _ => {}
        }
    }
}

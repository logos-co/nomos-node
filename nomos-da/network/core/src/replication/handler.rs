use std::io::Error;
use std::task::{Context, Poll};

use futures::Future;
use futures::future::BoxFuture;
use futures::prelude::*;
use libp2p::{PeerId, Stream, StreamProtocol};
use libp2p::core::upgrade::ReadyUpgrade;
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol};
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound};
use tracing::{debug, error};

use nomos_da_messages::{pack_message, unpack_from_reader};

use crate::protocol::PROTOCOL_NAME;

pub type DaMessage = nomos_da_messages::replication::ReplicationReq;

/// Events that bubbles up from the `BroadcastHandler` to the `NetworkBehaviour`
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
    pub fn new() -> Self {
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
    local_peer_id: PeerId,
    // incoming messages stream
    inbound: Option<BoxFuture<'static, Result<(DaMessage, Stream), Error>>>,
    // outgoing messages stream
    outbound: Option<OutboundState>,
    // pending messages not propagated in the connection
    outgoing_messages: Vec<DaMessage>,
}

impl ReplicationHandler {
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            inbound: None,
            outbound: None,
            outgoing_messages: Default::default(),
        }
    }
}

impl ReplicationHandler {
    fn send_pending_messages(
        &mut self,
        mut stream: Stream,
    ) -> impl Future<Output = Result<Stream, Error>> {
        let mut pending_messages = Vec::new();
        std::mem::swap(&mut self.outgoing_messages, &mut pending_messages);
        async {
            for message in pending_messages {
                stream.write_all(&pack_message(&message)?).await?;
            }
            stream.flush().await?;
            Ok(stream)
        }
    }

    fn read_message(
        &mut self,
        mut stream: Stream,
    ) -> impl Future<Output = Result<(DaMessage, Stream), Error>> {
        async move {
            let msg: DaMessage = unpack_from_reader(&mut stream).await?;
            Ok((msg, stream))
        }
    }

    fn poll_pending_incoming_messages(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Option<Result<DaMessage, Error>> {
        match self.inbound.take() {
            Some(mut fut) => {
                let mut read = std::pin::pin!(&mut fut);
                match read.poll_unpin(cx) {
                    Poll::Ready(Ok((message, stream))) => {
                        self.inbound = Some(self.read_message(stream).boxed());
                        Some(Ok(message))
                    }
                    Poll::Ready(Err(e)) => Some(Err(e)),
                    Poll::Pending => {
                        self.inbound = Some(fut);
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
    ) -> Result<
        Option<ConnectionHandlerEvent<ReadyUpgrade<StreamProtocol>, (), HandlerEventToBehaviour>>,
        Error,
    > {
        loop {
            // Propagate incoming messages
            match self.outbound.take() {
                Some(OutboundState::OpenStream) => {
                    self.outbound = Some(OutboundState::OpenStream);
                    break;
                }
                Some(OutboundState::Idle(stream)) => {
                    if !self.outgoing_messages.is_empty() {
                        self.outbound = Some(OutboundState::Sending(
                            self.send_pending_messages(stream).boxed(),
                        ));
                    } else {
                        self.outbound = Some(OutboundState::Idle(stream));
                    }
                    break;
                }
                Some(OutboundState::Sending(mut future)) => match future.poll_unpin(cx) {
                    Poll::Ready(Ok(stream)) => {
                        self.outbound = Some(OutboundState::Idle(stream));
                        break;
                    }
                    Poll::Ready(Err(e)) => {
                        error!("{e:?}");
                        return Err(e);
                    }
                    Poll::Pending => {}
                },
                None => {
                    self.outbound = Some(OutboundState::OpenStream);
                    return Ok(Some(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: <Self as ConnectionHandler>::listen_protocol(self),
                    }));
                }
            }
        }
        Ok(None)
    }
}
impl ConnectionHandler for ReplicationHandler {
    type FromBehaviour = BehaviourEventToHandler;
    type ToBehaviour = HandlerEventToBehaviour;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL_NAME), ())
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        match self.poll_pending_outgoing_messages(cx) {
            Ok(Some(event)) => {
                // bubble up event
                return Poll::Ready(event);
            }
            Err(error) => {
                error!("{error:?}");
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                    HandlerEventToBehaviour::OutgoingMessageError { error },
                ));
            }
            _ => {}
        };
        match self.poll_pending_incoming_messages(cx) {
            None => {}
            Some(Ok(message)) => {
                return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                    HandlerEventToBehaviour::IncomingMessage { message },
                ));
            }
            Some(Err(e)) => {
                debug!("Incoming DA message error {e}");
            }
        }
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            BehaviourEventToHandler::OutgoingMessage { message } => {
                self.outgoing_messages.push(message);
            }
        }
    }

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        let is_outbound = event.is_outbound();
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                info: _,
            }) => {
                debug!("{} - Got inbound stream", self.local_peer_id);
                self.inbound = Some(self.read_message(stream).boxed());
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                info: _,
            }) => {
                debug!("{} - Got outbound stream", self.local_peer_id);
                self.outbound = Some(OutboundState::Idle(stream));
            }
            ConnectionEvent::DialUpgradeError(error) => {
                debug!("{} - Dial error: [{error:?}]", self.local_peer_id);
                if is_outbound {
                    self.outbound = None;
                }
            }
            other => {
                debug!(
                    "{} - Connection event event = [{other:?}]",
                    self.local_peer_id
                );
            }
        }
    }
}

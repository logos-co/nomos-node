use std::collections::HashSet;
use std::io::Error;
use std::task::{Context, Poll};

use futures::Future;
use futures::prelude::*;
use libp2p::{Stream, StreamProtocol};
use libp2p::core::upgrade::ReadyUpgrade;
use libp2p::swarm::{ConnectionHandler, ConnectionHandlerEvent, SubstreamProtocol};
use libp2p::swarm::handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound};
use tracing::debug;

use crate::protocol::PROTOCOL_NAME;

// TODO: Hook real DA protocol message
pub type DaMessage = [u8; 1];

#[derive(Debug)]
pub enum HandlerEventToBehaviour {
    IncomingMessage { message: DaMessage },
    OutgoingMessageError { error: Error },
}

#[derive(Debug)]
pub enum BehaviourEventToHandler {
    OutgoingMessage { message: DaMessage },
}

pub(crate) struct BroadcastHandlerConfig {}

impl BroadcastHandlerConfig {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for BroadcastHandlerConfig {
    fn default() -> Self {
        Self::new()
    }
}

enum OutboundState {
    Idle(Stream),
    Sending(future::BoxFuture<'static, Result<Stream, Error>>),
}
pub struct BroadcastHandler {
    // incoming messages stream
    inbound: Option<Stream>,
    // outgoing messages stream
    outbound: Option<OutboundState>,
    // pending messages not propagated in the connection
    outgoing_messages: HashSet<DaMessage>,
    // pending errors on outgoing message
    pending_errors: Vec<Error>,
}

impl BroadcastHandler {
    pub fn new() -> Self {
        Self {
            inbound: None,
            outbound: None,
            outgoing_messages: Default::default(),
            pending_errors: vec![],
        }
    }
}

impl BroadcastHandler {
    fn send_pending_messages(
        &mut self,
        mut stream: Stream,
    ) -> impl Future<Output = Result<Stream, Error>> {
        let mut pending_messages = HashSet::new();
        std::mem::swap(&mut self.outgoing_messages, &mut pending_messages);
        async {
            for message in pending_messages {
                stream.write(&message).await?;
            }
            stream.flush().await?;
            Ok(stream)
        }
    }

    fn poll_pending_incoming_messages(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Option<Result<DaMessage, Error>> {
        self.inbound.as_mut().map(|stream| {
            let mut buff = DaMessage::default();
            loop {
                let mut read = stream.read_exact(&mut buff);
                match read.poll_unpin(cx) {
                    Poll::Ready(Ok(_)) => {
                        return Ok(buff);
                    }
                    Poll::Ready(Err(e)) => {
                        return Err(e);
                    }
                    Poll::Pending => {}
                }
            }
        })
    }

    fn poll_pending_outgoing_messages(&mut self, cx: &mut Context<'_>) -> Result<(), Error> {
        loop {
            // Propagate incoming messages
            match self.outbound.take() {
                Some(OutboundState::Idle(stream)) => {
                    self.outbound = Some(OutboundState::Sending(
                        self.send_pending_messages(stream).boxed(),
                    ));
                    break;
                }
                Some(OutboundState::Sending(mut future)) => match future.poll_unpin(cx) {
                    Poll::Ready(Ok(stream)) => {
                        self.outbound = Some(OutboundState::Idle(stream));
                        break;
                    }
                    Poll::Ready(Err(e)) => return Err(e),
                    Poll::Pending => {}
                },
                _ => {
                    break;
                }
            }
        }
        Ok(())
    }
}
impl ConnectionHandler for BroadcastHandler {
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
        if let Err(error) = self.poll_pending_outgoing_messages(cx) {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                HandlerEventToBehaviour::OutgoingMessageError { error },
            ));
        }
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
                self.outgoing_messages.insert(message);
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
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                info: _,
            }) => {
                self.inbound = Some(stream);
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                info: _,
            }) => {
                self.outbound = Some(OutboundState::Idle(stream));
            }
            ConnectionEvent::AddressChange(_) => {}
            ConnectionEvent::DialUpgradeError(_) => {}
            ConnectionEvent::ListenUpgradeError(_) => {}
            ConnectionEvent::LocalProtocolsChange(_) => {}
            ConnectionEvent::RemoteProtocolsChange(_) => {}
            _ => {}
        }
    }
}

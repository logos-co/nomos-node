use std::{
    collections::VecDeque,
    io,
    marker::PhantomData,
    task::{Context, Poll, Waker},
};

use futures::{future::BoxFuture, AsyncReadExt, AsyncWriteExt, FutureExt};
use libp2p::{
    core::upgrade::ReadyUpgrade,
    swarm::{
        handler::{ConnectionEvent, FullyNegotiatedInbound, FullyNegotiatedOutbound},
        ConnectionHandler, ConnectionHandlerEvent, StreamUpgradeError, SubstreamProtocol,
    },
    StreamProtocol,
};
use nomos_blend::conn_monitor::{ConnectionMonitor, ConnectionMonitorOutput};
use nomos_blend_message::BlendMessage;

// Metrics
const VALUE_FULLY_NEGOTIATED_INBOUND: &str = "fully_negotiated_inbound";
const VALUE_FULLY_NEGOTIATED_OUTBOUND: &str = "fully_negotiated_outbound";
const VALUE_DIAL_UPGRADE_ERROR: &str = "dial_upgrade_error";
const VALUE_IGNORED: &str = "ignored";

const PROTOCOL_NAME: StreamProtocol = StreamProtocol::new("/nomos/blend/0.1.0");

pub struct BlendConnectionHandler<Msg, Interval> {
    inbound_substream: Option<InboundSubstreamState>,
    outbound_substream: Option<OutboundSubstreamState>,
    outbound_msgs: VecDeque<Vec<u8>>,
    pending_events_to_behaviour: VecDeque<ToBehaviour>,
    monitor: Option<ConnectionMonitor<Interval>>,
    waker: Option<Waker>,
    _blend_message: PhantomData<Msg>,
}

type MsgSendFuture = BoxFuture<'static, Result<libp2p::Stream, io::Error>>;
type MsgRecvFuture = BoxFuture<'static, Result<(libp2p::Stream, Vec<u8>), io::Error>>;

enum InboundSubstreamState {
    /// A message is being received on the inbound substream.
    PendingRecv(MsgRecvFuture),
    /// A substream has been dropped proactively.
    Dropped,
}

enum OutboundSubstreamState {
    /// A request to open a new outbound substream is being processed.
    PendingOpenSubstream,
    /// An outbound substream is open and ready to send messages.
    Idle(libp2p::Stream),
    /// A message is being sent on the outbound substream.
    PendingSend(MsgSendFuture),
    /// A substream has been dropped proactively.
    Dropped,
}

impl<Msg, Interval> BlendConnectionHandler<Msg, Interval> {
    pub fn new(monitor: Option<ConnectionMonitor<Interval>>) -> Self {
        Self {
            inbound_substream: None,
            outbound_substream: None,
            outbound_msgs: VecDeque::new(),
            pending_events_to_behaviour: VecDeque::new(),
            monitor,
            waker: None,
            _blend_message: PhantomData,
        }
    }

    fn try_wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

#[derive(Debug)]
pub enum FromBehaviour {
    /// A message to be sent to the connection.
    Message(Vec<u8>),
}

#[derive(Debug)]
pub enum ToBehaviour {
    /// An outbound substream has been successfully upgraded for the blend protocol.
    FullyNegotiatedOutbound,
    /// An outbound substream was failed to be upgraded for the blend protocol.
    NegotiationFailed,
    /// A message has been received from the connection.
    Message(Vec<u8>),
    /// A result of connection monitoring
    MaliciousPeer,
    UnhealthyPeer,
    /// An IO error from the connection
    IOError(io::Error),
}

impl<Msg, Interval> ConnectionHandler for BlendConnectionHandler<Msg, Interval>
where
    Msg: BlendMessage + Send + 'static,
    Interval: futures::Stream + Unpin + Send + 'static,
{
    type FromBehaviour = FromBehaviour;
    type ToBehaviour = ToBehaviour;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type InboundOpenInfo = ();
    type OutboundProtocol = ReadyUpgrade<StreamProtocol>;
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
        tracing::info!(gauge.pending_outbound_messages = self.outbound_msgs.len() as u64,);
        tracing::info!(
            gauge.pending_events_to_behaviour = self.pending_events_to_behaviour.len() as u64,
        );

        // Check if the monitor interval has elapsed, if exists.
        if let Some(monitor) = &mut self.monitor {
            if let Poll::Ready(output) = monitor.poll(cx) {
                match output {
                    ConnectionMonitorOutput::Malicious => {
                        // Mark the inbound/outbound substreams as Dropped to drop them from memory.
                        // Then, the reference count to this connection will decrease.
                        // Swarm will close the connection when there is no active stream anymore.
                        self.inbound_substream = Some(InboundSubstreamState::Dropped);
                        self.outbound_substream = Some(OutboundSubstreamState::Dropped);
                        self.pending_events_to_behaviour
                            .push_back(ToBehaviour::MaliciousPeer);
                    }
                    ConnectionMonitorOutput::Unhealthy => {
                        self.pending_events_to_behaviour
                            .push_back(ToBehaviour::UnhealthyPeer);
                    }
                    ConnectionMonitorOutput::Healthy => {}
                }
            }
        }

        // Process pending events to be sent to the behaviour
        if let Some(event) = self.pending_events_to_behaviour.pop_front() {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(event));
        }

        // Process inbound stream
        tracing::debug!("Processing inbound stream");
        match self.inbound_substream.take() {
            None => {
                tracing::debug!("Inbound substream is not initialized yet. Doing nothing.");
                self.inbound_substream = None;
            }
            Some(InboundSubstreamState::PendingRecv(mut msg_recv_fut)) => match msg_recv_fut
                .poll_unpin(cx)
            {
                Poll::Ready(Ok((stream, msg))) => {
                    tracing::debug!("Received message from inbound stream. Notifying behaviour...");

                    if let Some(monitor) = &mut self.monitor {
                        if Msg::is_drop_message(&msg) {
                            monitor.record_drop_message();
                        } else {
                            monitor.record_effective_message();
                        }
                    }

                    self.inbound_substream =
                        Some(InboundSubstreamState::PendingRecv(recv_msg(stream).boxed()));
                    return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                        ToBehaviour::Message(msg),
                    ));
                }
                Poll::Ready(Err(e)) => {
                    tracing::error!(
                        "Failed to receive message from inbound stream: {e:?}. Dropping both inbound/outbound substreams"
                    );
                    self.inbound_substream = Some(InboundSubstreamState::Dropped);
                    self.outbound_substream = Some(OutboundSubstreamState::Dropped);
                }
                Poll::Pending => {
                    tracing::debug!("No message received from inbound stream yet. Waiting more...");
                    self.inbound_substream = Some(InboundSubstreamState::PendingRecv(msg_recv_fut));
                }
            },
            Some(InboundSubstreamState::Dropped) => {
                tracing::debug!("Inbound substream has been dropped proactively. Doing nothing.");
                self.inbound_substream = Some(InboundSubstreamState::Dropped);
            }
        }

        // Process outbound stream
        tracing::debug!("Processing outbound stream");
        loop {
            match self.outbound_substream.take() {
                // If the request to open a new outbound substream is still being processed, wait more.
                Some(OutboundSubstreamState::PendingOpenSubstream) => {
                    self.outbound_substream = Some(OutboundSubstreamState::PendingOpenSubstream);
                    self.waker = Some(cx.waker().clone());
                    return Poll::Pending;
                }
                // If the substream is idle, and if it's time to send a message, send it.
                Some(OutboundSubstreamState::Idle(stream)) => {
                    match self.outbound_msgs.pop_front() {
                        Some(msg) => {
                            tracing::debug!("Sending message to outbound stream: {:?}", msg);
                            self.outbound_substream = Some(OutboundSubstreamState::PendingSend(
                                send_msg(stream, msg).boxed(),
                            ));
                        }
                        None => {
                            tracing::debug!("Nothing to send to outbound stream");
                            self.outbound_substream = Some(OutboundSubstreamState::Idle(stream));
                            self.waker = Some(cx.waker().clone());
                            return Poll::Pending;
                        }
                    }
                }
                // If a message is being sent, check if it's done.
                Some(OutboundSubstreamState::PendingSend(mut msg_send_fut)) => {
                    match msg_send_fut.poll_unpin(cx) {
                        Poll::Ready(Ok(stream)) => {
                            tracing::debug!("Message sent to outbound stream");
                            self.outbound_substream = Some(OutboundSubstreamState::Idle(stream));
                        }
                        Poll::Ready(Err(e)) => {
                            tracing::error!("Failed to send message to outbound stream: {e:?}. Dropping both inbound and outbound substreams");
                            self.outbound_substream = Some(OutboundSubstreamState::Dropped);
                            self.inbound_substream = Some(InboundSubstreamState::Dropped);
                            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                                ToBehaviour::IOError(e),
                            ));
                        }
                        Poll::Pending => {
                            self.outbound_substream =
                                Some(OutboundSubstreamState::PendingSend(msg_send_fut));
                            self.waker = Some(cx.waker().clone());
                            return Poll::Pending;
                        }
                    }
                }
                Some(OutboundSubstreamState::Dropped) => {
                    tracing::debug!("Outbound substream has been dropped proactively");
                    self.outbound_substream = Some(OutboundSubstreamState::Dropped);
                    return Poll::Pending;
                }
                // If there is no outbound substream yet, request to open a new one.
                None => {
                    self.outbound_substream = Some(OutboundSubstreamState::PendingOpenSubstream);
                    return Poll::Ready(ConnectionHandlerEvent::OutboundSubstreamRequest {
                        protocol: SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL_NAME), ()),
                    });
                }
            }
        }
    }

    fn on_behaviour_event(&mut self, event: Self::FromBehaviour) {
        match event {
            FromBehaviour::Message(msg) => {
                self.outbound_msgs.push_back(msg);
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
        let event_name = match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol: stream,
                ..
            }) => {
                tracing::debug!("FullyNegotiatedInbound: Creating inbound substream");
                self.inbound_substream =
                    Some(InboundSubstreamState::PendingRecv(recv_msg(stream).boxed()));
                VALUE_FULLY_NEGOTIATED_INBOUND
            }
            ConnectionEvent::FullyNegotiatedOutbound(FullyNegotiatedOutbound {
                protocol: stream,
                ..
            }) => {
                tracing::debug!("FullyNegotiatedOutbound: Creating outbound substream");
                self.outbound_substream = Some(OutboundSubstreamState::Idle(stream));
                self.pending_events_to_behaviour
                    .push_back(ToBehaviour::FullyNegotiatedOutbound);
                VALUE_FULLY_NEGOTIATED_OUTBOUND
            }
            ConnectionEvent::DialUpgradeError(e) => {
                tracing::error!("DialUpgradeError: {:?}", e);
                match e.error {
                    StreamUpgradeError::NegotiationFailed => {
                        self.pending_events_to_behaviour
                            .push_back(ToBehaviour::NegotiationFailed);
                    }
                    StreamUpgradeError::Io(e) => {
                        self.pending_events_to_behaviour
                            .push_back(ToBehaviour::IOError(e));
                    }
                    StreamUpgradeError::Timeout => {
                        self.pending_events_to_behaviour
                            .push_back(ToBehaviour::IOError(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "blend protocol negotiation timed out",
                            )));
                    }
                    StreamUpgradeError::Apply(_) => unreachable!(),
                };
                VALUE_DIAL_UPGRADE_ERROR
            }
            event => {
                tracing::debug!("Ignoring connection event: {:?}", event);
                VALUE_IGNORED
            }
        };

        tracing::info!(counter.connection_event = 1, event = event_name);
        self.try_wake();
    }
}

/// Write a message to the stream
async fn send_msg(mut stream: libp2p::Stream, msg: Vec<u8>) -> io::Result<libp2p::Stream> {
    let msg_len: u16 = msg.len().try_into().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "Message length is too big. Got {}, expected {}",
                msg.len(),
                std::mem::size_of::<u16>()
            ),
        )
    })?;
    stream.write_all(msg_len.to_be_bytes().as_ref()).await?;
    stream.write_all(&msg).await?;
    stream.flush().await?;
    Ok(stream)
}
/// Read a message from the stream
async fn recv_msg(mut stream: libp2p::Stream) -> io::Result<(libp2p::Stream, Vec<u8>)> {
    let mut msg_len = [0; std::mem::size_of::<u16>()];
    stream.read_exact(&mut msg_len).await?;
    let msg_len = u16::from_be_bytes(msg_len) as usize;

    let mut buf = vec![0; msg_len];
    stream.read_exact(&mut buf).await?;
    Ok((stream, buf))
}

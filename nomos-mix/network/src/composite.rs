use std::{
    collections::VecDeque,
    task::{Context, Poll},
};

use either::Either;
use libp2p::{
    core::Endpoint,
    gossipsub,
    swarm::{
        ConnectionDenied, ConnectionHandler, ConnectionHandlerSelect, ConnectionId, FromSwarm,
        NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};

use crate::{
    behaviour::{MixBehaviour, MixBehaviourEvent},
    error::Error,
};

/// The topic used for gossipsubing fully unwrapped messages (exited from mix nodes).
const FULLY_UNWRAPPED_MSG_TOPIC: &str = "nomos-mix/fully-unwrapped-msg";

/// A composite behaviour of the [`MixBehaviour`] and [`gossipsub::Behaviour`].
///
/// This behaviour mixes messages using mix nodes and broadcasts them using gossipsub.
pub struct Behaviour {
    mix: MixBehaviour,
    gossipsub: gossipsub::Behaviour,
    topic: gossipsub::IdentTopic,
    pending_events: VecDeque<ToSwarm<Event, THandlerInEvent<Self>>>,
}

/// Events that are yielded by the [`Behaviour`].
#[derive(Debug)]
pub enum Event {
    /// Events from the [`MixBehaviour`]
    Mix(MixBehaviourEvent),
    /// Events from the [`gossipsub::Behaviour`]
    Gossipsub(gossipsub::Event),
    /// An error event
    Error(Error),
}

impl From<MixBehaviourEvent> for Event {
    fn from(event: MixBehaviourEvent) -> Self {
        Self::Mix(event)
    }
}

impl From<gossipsub::Event> for Event {
    fn from(event: gossipsub::Event) -> Self {
        Self::Gossipsub(event)
    }
}

impl Behaviour {
    // TODO: Accept config instead of [`MixBehaviour`].
    pub fn new(mix: MixBehaviour, mut gossipsub: gossipsub::Behaviour) -> Result<Self, Error> {
        let topic = gossipsub::IdentTopic::new(FULLY_UNWRAPPED_MSG_TOPIC);
        gossipsub
            .subscribe(&topic)
            .map_err(Error::GossipsubSubscriptionError)?;
        Ok(Self {
            mix,
            gossipsub,
            topic,
            pending_events: VecDeque::new(),
        })
    }

    /// Publish a message to the network.
    ///
    /// The message will be mixed first by mix nodes and then broadcasted using gossipsub.
    pub fn publish(&mut self, message: Vec<u8>) -> Result<(), Error> {
        self.mix.publish(message)
    }
}

impl NetworkBehaviour for Behaviour {
    // Use [`ConnectionHandlerSelect`] to support both protocols.
    type ConnectionHandler = ConnectionHandlerSelect<
        <MixBehaviour as NetworkBehaviour>::ConnectionHandler,
        <gossipsub::Behaviour as NetworkBehaviour>::ConnectionHandler,
    >;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.mix
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)?;
        self.gossipsub
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mix_handler = self.mix.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        let gossipsub_handler = self.gossipsub.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        Ok(mix_handler.select(gossipsub_handler))
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let mut combined_addresses = Vec::new();
        combined_addresses.extend(self.mix.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?);
        combined_addresses.extend(self.gossipsub.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?);
        Ok(combined_addresses)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let mix_handler = self.mix.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )?;
        let gossipsub_handler = self.gossipsub.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )?;
        Ok(mix_handler.select(gossipsub_handler))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.mix.on_swarm_event(event);
        self.gossipsub.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            Either::Left(event) => {
                self.mix
                    .on_connection_handler_event(peer_id, connection_id, event)
            }
            Either::Right(event) => {
                self.gossipsub
                    .on_connection_handler_event(peer_id, connection_id, event)
            }
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        // Handle pending events first
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(event);
        }

        // Handle mix events
        if let Poll::Ready(event) = self.mix.poll(cx) {
            match &event {
                ToSwarm::GenerateEvent(MixBehaviourEvent::FullyUnwrappedMessage(msg)) => {
                    if let Err(e) = self.gossipsub.publish(self.topic.clone(), msg.clone()) {
                        // Schedule an error event.
                        self.pending_events.push_back(
                            ToSwarm::GenerateEvent(Event::Error(Error::GossipsubPublishError(e)))
                                .map_in(Either::Left),
                        );
                    }
                    // Return the fully unwrapped message even though it was published by gossipsub
                    // because the gossipsub publisher doesn't receive the message.
                    return Poll::Ready(event.map_out(|e| e.into()).map_in(Either::Left));
                }
                _ => {
                    return Poll::Ready(event.map_out(|e| e.into()).map_in(Either::Left));
                }
            }
        }

        // Handle gossipsub events
        if let Poll::Ready(event) = self.gossipsub.poll(cx) {
            return Poll::Ready(event.map_out(|e| e.into()).map_in(Either::Right));
        }

        Poll::Pending
    }
}

use crate::protocol::DISPERSAL_PROTOCOL;
use crate::SubnetworkId;
use either::Either;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::{AsyncWriteExt, FutureExt, StreamExt};
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
    THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId, Stream};
use libp2p_stream::IncomingStreams;
use log::debug;
use nomos_da_messages::dispersal::dispersal_res::MessageType;
use nomos_da_messages::dispersal::{DispersalReq, DispersalRes};
use nomos_da_messages::{pack_message, unpack_from_reader};
use std::io::Error;
use std::task::{Context, Poll};
use subnetworks_assignations::MembershipHandler;

#[derive(Debug)]
pub enum DispersalEvent {
    /// Received a n
    IncomingMessage { message: DispersalReq },
}
pub struct DispersalValidatorBehaviour<Membership> {
    stream_behaviour: libp2p_stream::Behaviour,
    incoming_streams: IncomingStreams,
    tasks: FuturesUnordered<BoxFuture<'static, Result<(DispersalReq, Stream), Error>>>,
    membership: Membership,
}

impl<Membership: MembershipHandler> DispersalValidatorBehaviour<Membership> {
    pub fn new(membership: Membership) -> Self {
        let stream_behaviour = libp2p_stream::Behaviour::new();
        let mut stream_control = stream_behaviour.new_control();
        let incoming_streams = stream_control
            .accept(DISPERSAL_PROTOCOL)
            .expect("Just a single accept to protocol is valid");
        let tasks = FuturesUnordered::new();
        Self {
            stream_behaviour,
            incoming_streams,
            tasks,
            membership,
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        self.membership = membership;
    }

    /// Stream handling messages task.
    /// This task handles a single message receive. Then it writes up the acknowledgment into the same
    /// stream as response and finish.
    async fn handle_new_stream(mut stream: Stream) -> Result<(DispersalReq, Stream), Error> {
        let message: DispersalReq = unpack_from_reader(&mut stream).await?;
        let blob_id = message.blob.clone().unwrap().blob_id;
        let response = DispersalRes {
            message_type: Some(MessageType::BlobId(blob_id)),
        };
        let message_bytes = pack_message(&response)?;
        stream.write_all(&message_bytes).await?;
        stream.flush().await?;
        Ok((message, stream))
    }
}

impl<M: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static> NetworkBehaviour
    for DispersalValidatorBehaviour<M>
{
    type ConnectionHandler = Either<
        <libp2p_stream::Behaviour as NetworkBehaviour>::ConnectionHandler,
        libp2p::swarm::dummy::ConnectionHandler,
    >;
    type ToSwarm = DispersalEvent;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.membership.is_allowed(&peer) {
            return Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler));
        }
        self.stream_behaviour
            .handle_established_inbound_connection(connection_id, peer, local_addr, remote_addr)
            .map(Either::Left)
    }

    fn handle_established_outbound_connection(
        &mut self,
        _connection_id: ConnectionId,
        _peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.stream_behaviour.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let Either::Left(event) = event else {
            unreachable!()
        };
        self.stream_behaviour
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        let Self {
            incoming_streams,
            tasks,
            ..
        } = self;
        match tasks.poll_next_unpin(cx) {
            Poll::Ready(Some(Ok((message, stream)))) => {
                tasks.push(Self::handle_new_stream(stream).boxed());
                return Poll::Ready(ToSwarm::GenerateEvent(DispersalEvent::IncomingMessage {
                    message,
                }));
            }
            Poll::Ready(Some(Err(error))) => {
                debug!("Error on dispersal stream {error:?}");
            }
            _ => {}
        }
        if let Poll::Ready(Some((_peer_id, stream))) = incoming_streams.poll_next_unpin(cx) {
            tasks.push(Self::handle_new_stream(stream).boxed());
        }
        // TODO: probably must be smarter when to wake this
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocols::dispersal::executor::behaviour::DispersalError;
    use crate::protocols::replication::handler::DaMessage;
    use futures::stream::BoxStream;
    use futures::task::ArcWake;
    use libp2p::{identity, PeerId};
    use libp2p_stream::Control;
    use nomos_da_messages::common::Blob;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::UnboundedReceiverStream;

    #[derive(Clone, Debug)]
    struct MockMembershipHandler {
        membership: HashMap<PeerId, HashSet<SubnetworkId>>,
    }

    impl MembershipHandler for MockMembershipHandler {
        type NetworkId = SubnetworkId;
        type Id = PeerId;

        fn membership(&self, peer_id: &PeerId) -> HashSet<Self::NetworkId> {
            self.membership.get(peer_id).cloned().unwrap_or_default()
        }

        fn members_of(&self, subnetwork: &Self::NetworkId) -> HashSet<Self::Id> {
            self.membership
                .iter()
                .filter_map(|(id, nets)| {
                    if nets.contains(subnetwork) {
                        Some(*id)
                    } else {
                        None
                    }
                })
                .collect()
        }

        fn is_allowed(&self, _id: &Self::Id) -> bool {
            if self.membership.contains_key(_id) {
                return true;
            }
            false
        }

        fn members(&self) -> HashSet<Self::Id> {
            let mut members = HashSet::new();
            for id in self.membership.keys() {
                members.insert(*id);
            }
            members
        }
    }

    struct TestWaker;

    impl ArcWake for TestWaker {
        fn wake_by_ref(_arc_self: &Arc<Self>) {}
    }

    fn create_validation_behaviours(
        num_instances: usize,
        subnet_id: u32,
        membership: &mut HashMap<PeerId, HashSet<SubnetworkId>>,
    ) -> Vec<DispersalValidatorBehaviour<MockMembershipHandler>> {
        let mut behaviours = Vec::new();

        let mut peer_ids = Vec::new();
        for _ in 0..num_instances {
            let keypair = identity::Keypair::generate_ed25519();
            let peer_id = PeerId::from(keypair.public());
            peer_ids.push(peer_id);
        }

        for peer_id in &peer_ids {
            membership.insert(*peer_id, HashSet::from([subnet_id]));
        }

        let membership_handler = MockMembershipHandler {
            membership: HashMap::default(), // This will be updated after all behaviours are added.
        };

        for _ in peer_ids {
            let behaviour = DispersalValidatorBehaviour::new(membership_handler.clone());
            behaviours.push(behaviour);
        }

        behaviours
    }

    fn establish_connection(
        behaviours: &mut [DispersalValidatorBehaviour<MockMembershipHandler>],
        i: usize,
        j: usize,
        connection_id: ConnectionId,
    ) -> BoxStream<DaMessage> {
        let mut members: Vec<PeerId> = behaviours[i].membership.members().into_iter().collect();
        members.sort();
        let peer_id_i = members[i];
        let peer_id_j = members[j];

        behaviours[i]
            .handle_established_outbound_connection(
                connection_id,
                peer_id_j,
                &Multiaddr::empty(),
                Endpoint::Dialer,
            )
            .unwrap();

        behaviours[j]
            .handle_established_inbound_connection(
                connection_id,
                peer_id_i,
                &Multiaddr::empty(),
                &Multiaddr::empty(),
            )
            .unwrap();

        let mut stream_control = behaviours[i].stream_behaviour.new_control();
        let (pending_out_streams_sender, receiver) = mpsc::unbounded_channel::<PeerId>();
        let pending_out_streams = UnboundedReceiverStream::new(receiver)
            .zip(futures::stream::repeat(stream_control))
            .then(|(peer_id, control)| open_stream(peer_id_j, control))
            .boxed();

        let (pending_blobs_sender, receiver) = mpsc::unbounded_channel::<DaMessage>();
        let pending_blobs_stream = UnboundedReceiverStream::new(receiver).boxed();

        pending_blobs_stream
    }

    async fn open_stream(peer_id: PeerId, mut control: Control) -> Result<Stream, DispersalError> {
        let stream = control
            .open_stream(peer_id, DISPERSAL_PROTOCOL)
            .await
            .map_err(|error| DispersalError::OpenStreamError { peer_id, error })?;
        Ok(stream)
    }

    #[test]
    fn test_handle_established_inbound_connection() {
        let mut allowed_peers = HashMap::new();
        allowed_peers.insert(PeerId::random(), HashSet::from([0, 1]));
        let membership = MockMembershipHandler {
            membership: allowed_peers,
        };
        let mut behaviour = DispersalValidatorBehaviour::new(membership);

        let allowed_peer = *behaviour.membership.members().iter().next().unwrap();
        let disallowed_peer = PeerId::random();
        let local_addr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
        let remote_addr = "/ip4/127.0.0.1/tcp/8081".parse().unwrap();

        let result = behaviour.handle_established_inbound_connection(
            ConnectionId::new_unchecked(0),
            allowed_peer,
            &local_addr,
            &remote_addr,
        );
        assert!(matches!(result, Ok(Either::Left(_))));

        let result = behaviour.handle_established_inbound_connection(
            ConnectionId::new_unchecked(1),
            disallowed_peer,
            &local_addr,
            &remote_addr,
        );
        assert!(matches!(result, Ok(Either::Right(_))));
    }

    #[test]
    fn test_poll() {
        let membership = MockMembershipHandler {
            membership: HashMap::new(),
        };
        let mut behaviour = DispersalValidatorBehaviour::new(membership);

        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        let poll_result = behaviour.poll(&mut cx);

        assert!(matches!(poll_result, Poll::Pending));
    }

    #[tokio::test]
    async fn test_validation_behaviour() {
        let num_instances = 20;
        let mut membership = HashMap::default();

        let subnet_0_behaviours =
            create_validation_behaviours(num_instances / 2, 0, &mut membership);
        let subnet_1_behaviours =
            create_validation_behaviours(num_instances / 2, 1, &mut membership);

        let mut all_behaviours = subnet_0_behaviours;
        all_behaviours.extend(subnet_1_behaviours);

        for behaviour in all_behaviours.iter_mut() {
            let membership_handler = MockMembershipHandler {
                membership: membership.clone(),
            };
            behaviour.update_membership(membership_handler);
        }

        let message = DaMessage {
            blob: Some(Blob {
                blob_id: vec![1, 2, 3],
                data: vec![4, 5, 6],
            }),
            subnetwork_id: 0,
        };

        // Simulate peer connections.
        for (i, j) in (0..num_instances).flat_map(|i| (i + 1..num_instances).map(move |j| (i, j))) {
            let connection_id = ConnectionId::new_unchecked(i);
            let stream_sender = establish_connection(&mut all_behaviours, i, j, connection_id);
        }
    }
}

// std
use std::collections::{HashMap, HashSet, VecDeque};
use std::task::{Context, Poll, Waker};

// crates
use either::Either;
use indexmap::IndexSet;
use libp2p::core::Endpoint;
use libp2p::swarm::{
    ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, NotifyHandler,
    THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
};
use libp2p::{Multiaddr, PeerId};
use log::{error, trace};
use subnetworks_assignations::MembershipHandler;

use crate::SubnetworkId;

// internal
use super::handler::{
    BehaviourEventToHandler, DaMessage, HandlerEventToBehaviour, ReplicationHandler,
};

type SwarmEvent = ToSwarm<ReplicationEvent, Either<BehaviourEventToHandler, void::Void>>;

/// Nomos DA BroadcastEvents to be bubble up to logic layers
#[allow(dead_code)] // todo: remove when used in tests
#[derive(Debug)]
pub enum ReplicationEvent {
    IncomingMessage { peer_id: PeerId, message: DaMessage },
}

impl ReplicationEvent {
    pub fn blob_size(&self) -> Option<usize> {
        match self {
            ReplicationEvent::IncomingMessage { message, .. } => {
                Some(message.blob.data.column_len())
            }
        }
    }
}

/// Nomos DA broadcas network behaviour
/// This item handles the logic of the nomos da subnetworks broadcasting
/// DA subnetworks are a logical distribution of subsets.
/// A node just connects and accepts connections to other nodes that are in the same subsets.
/// A node forwards messages to all connected peers which are member of the addressed `SubnetworkId`.
pub struct ReplicationBehaviour<Membership> {
    /// Local peer Id, related to the libp2p public key
    local_peer_id: PeerId,
    /// Membership handler, membership handles the subsets logics on who is where in the
    /// nomos DA subnetworks
    membership: Membership,
    /// Relation of connected peers of replication subnetworks
    ///
    /// **TODO**: Node needs only one connection per peer for Nomos DA network communications.
    /// Allowing multiple connections from the same peer id is only temporal and will be removed!
    connected: HashMap<PeerId, HashSet<ConnectionId>>,
    /// Outgoing event queue
    outgoing_events: VecDeque<SwarmEvent>,
    /// Seen messages cache holds a record of seen messages, messages will be removed from this
    /// set after some time to keep it
    seen_message_cache: IndexSet<(Vec<u8>, SubnetworkId)>,
    /// Waker that handles polling
    waker: Option<Waker>,
}

impl<Membership> ReplicationBehaviour<Membership> {
    pub fn new(peer_id: PeerId, membership: Membership) -> Self {
        Self {
            local_peer_id: peer_id,
            membership,
            connected: Default::default(),
            outgoing_events: Default::default(),
            seen_message_cache: Default::default(),
            waker: None,
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        self.membership = membership;
    }
}

impl<M> ReplicationBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    /// Check if some peer membership lies in at least a single subnetwork that the local peer is a
    /// member too.
    fn is_neighbour(&self, peer_id: &PeerId) -> bool {
        self.membership
            .membership(&self.local_peer_id)
            .intersection(&self.membership.membership(peer_id))
            .count()
            > 0
    }

    fn no_loopback_member_peers_of(&self, subnetwork: &SubnetworkId) -> HashSet<PeerId> {
        let mut peers = self.membership.members_of(subnetwork);
        // no loopback
        peers.remove(&self.local_peer_id);
        peers
    }

    fn replicate_message(&mut self, message: DaMessage) {
        let message_id = (message.blob.blob_id.to_vec(), message.subnetwork_id);
        if self.seen_message_cache.contains(&message_id) {
            return;
        }
        self.seen_message_cache.insert(message_id);
        self.send_message(message)
    }

    pub fn send_message(&mut self, message: DaMessage) {
        // push a message in the queue for every single peer connected that is a member of the
        // selected subnetwork_id
        let peers = self.no_loopback_member_peers_of(&message.subnetwork_id);

        let connected_peers: Vec<_> = self
            .connected
            .iter()
            .filter(|(peer_id, _connection_id)| peers.contains(peer_id))
            .collect();

        for (peer_id, connection_ids) in connected_peers {
            for connection_id in connection_ids.iter() {
                self.outgoing_events.push_back(SwarmEvent::NotifyHandler {
                    peer_id: *peer_id,
                    handler: NotifyHandler::One(*connection_id),
                    event: Either::Left(BehaviourEventToHandler::OutgoingMessage {
                        message: message.clone(),
                    }),
                })
            }
        }
        self.try_wake();
    }

    pub fn try_wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

impl<M> NetworkBehaviour for ReplicationBehaviour<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
{
    type ConnectionHandler = Either<ReplicationHandler, libp2p::swarm::dummy::ConnectionHandler>;
    type ToSwarm = ReplicationEvent;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        if !self.is_neighbour(&peer_id) {
            trace!("refusing connection to {peer_id}");
            return Ok(Either::Right(libp2p::swarm::dummy::ConnectionHandler));
        }
        trace!("{}, Connected to {peer_id}", self.local_peer_id);
        self.connected
            .entry(peer_id)
            .or_default()
            .insert(connection_id);
        Ok(Either::Left(ReplicationHandler::new()))
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer_id: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        trace!("{}, Connected to {peer_id}", self.local_peer_id);
        self.connected
            .entry(peer_id)
            .or_default()
            .insert(connection_id);
        Ok(Either::Left(ReplicationHandler::new()))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            connection_id,
            ..
        }) = event
        {
            if let Some(connections) = self.connected.get_mut(&peer_id) {
                connections.remove(&connection_id);
                if connections.is_empty() {
                    self.connected.remove(&peer_id);
                }
            }
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        _connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        let event = match event {
            Either::Left(e) => e,
            Either::Right(v) => void::unreachable(v),
        };
        match event {
            HandlerEventToBehaviour::IncomingMessage { message } => {
                self.replicate_message(message.clone());
                self.outgoing_events.push_back(ToSwarm::GenerateEvent(
                    ReplicationEvent::IncomingMessage { message, peer_id },
                ));
            }
            HandlerEventToBehaviour::OutgoingMessageError { error } => {
                error!("Couldn't send message due to {error}");
                todo!("Retry?")
            }
        }
        self.try_wake();
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(event) = self.outgoing_events.pop_front() {
            Poll::Ready(event)
        } else {
            self.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::task::{waker_ref, ArcWake};
    use kzgrs_backend::common::blob::DaBlob;
    use kzgrs_backend::encoder;
    use kzgrs_backend::encoder::DaEncoderParams;
    use libp2p::{identity, PeerId};
    use nomos_core::da::{BlobId, DaEncoder};
    use nomos_da_messages::common::Blob;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::task::{Context, Poll};

    fn get_encoder() -> encoder::DaEncoder {
        const DOMAIN_SIZE: usize = 16;
        let params = DaEncoderParams::default_with(DOMAIN_SIZE);
        encoder::DaEncoder::new(params)
    }

    fn get_da_blob() -> DaBlob {
        let encoder = get_encoder();
        let data = vec![
            49u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        ];

        let encoded_data = encoder.encode(&data).unwrap();
        let columns: Vec<_> = encoded_data.extended_data.columns().collect();

        let index = 0;
        let da_blob = DaBlob {
            column: columns[index].clone(),
            column_idx: index
                .try_into()
                .expect("Column index shouldn't overflow the target type"),
            column_commitment: encoded_data.column_commitments[index],
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            aggregated_column_proof: encoded_data.aggregated_column_proofs[index],
            rows_commitments: encoded_data.row_commitments.clone(),
            rows_proofs: encoded_data
                .rows_proofs
                .iter()
                .map(|proofs| proofs.get(index).cloned().unwrap())
                .collect(),
        };

        da_blob
    }

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
            unimplemented!()
        }

        fn members(&self) -> HashSet<Self::Id> {
            unimplemented!()
        }
    }

    struct TestWaker;

    impl ArcWake for TestWaker {
        fn wake_by_ref(_arc_self: &Arc<Self>) {}
    }

    fn create_replication_behaviours(
        num_instances: usize,
        subnetwork_id: SubnetworkId,
        membership: &mut HashMap<PeerId, HashSet<SubnetworkId>>,
    ) -> Vec<ReplicationBehaviour<MockMembershipHandler>> {
        let mut behaviours = Vec::new();

        let mut peer_ids = Vec::new();
        for _ in 0..num_instances {
            let keypair = identity::Keypair::generate_ed25519();
            let peer_id = PeerId::from(keypair.public());
            peer_ids.push(peer_id);
        }

        for peer_id in &peer_ids {
            membership.insert(*peer_id, HashSet::from([subnetwork_id]));
        }

        let membership_handler = MockMembershipHandler {
            membership: HashMap::default(), // This will be updated after all behaviours are added.
        };

        for peer_id in peer_ids {
            let behaviour = ReplicationBehaviour::new(peer_id, membership_handler.clone());
            behaviours.push(behaviour);
        }

        behaviours
    }

    fn establish_connection(
        behaviours: &mut [ReplicationBehaviour<MockMembershipHandler>],
        i: usize,
        j: usize,
        connection_id: ConnectionId,
    ) {
        let peer_id_i = behaviours[i].local_peer_id;
        let peer_id_j = behaviours[j].local_peer_id;

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
    }

    fn deliver_message_to_peer(
        all_behaviours: &mut [ReplicationBehaviour<MockMembershipHandler>],
        peer_id: PeerId,
        connection_id: ConnectionId,
        message: DaMessage,
    ) {
        if let Some(behaviour) = all_behaviours
            .iter_mut()
            .find(|b| b.local_peer_id == peer_id)
        {
            // Simulate the handler receiving the message.
            behaviour.on_connection_handler_event(
                peer_id,
                connection_id,
                Either::Left(HandlerEventToBehaviour::IncomingMessage { message }),
            );
        }
    }

    #[tokio::test]
    async fn test_replication_behaviour() {
        let num_instances = 20;
        let mut membership = HashMap::default();

        let subnet_0_behaviours =
            create_replication_behaviours(num_instances / 2, 0, &mut membership);
        let subnet_1_behaviours =
            create_replication_behaviours(num_instances / 2, 1, &mut membership);

        let mut all_behaviours = subnet_0_behaviours;
        all_behaviours.extend(subnet_1_behaviours);

        for behaviour in all_behaviours.iter_mut() {
            let membership_handler = MockMembershipHandler {
                membership: membership.clone(),
            };
            behaviour.update_membership(membership_handler);
        }

        // Simulate peer connections.
        for (i, j) in (0..num_instances).flat_map(|i| (i + 1..num_instances).map(move |j| (i, j))) {
            let connection_id = ConnectionId::new_unchecked(i);
            establish_connection(&mut all_behaviours, i, j, connection_id);
        }

        // Simulate sending a message from the first behavior.
        let message = DaMessage::new(Blob::new(BlobId::from([0; 32]), get_da_blob()), 0);
        all_behaviours[0].replicate_message(message.clone());

        let waker = Arc::new(TestWaker);
        let waker_ref = waker_ref(&waker);
        let mut cx = Context::from_waker(&waker_ref);

        // Poll all behaviors until no more events are generated.
        let mut pending_behaviours: Vec<_> = (0..num_instances).collect();
        let mut completed = false;

        while !completed {
            completed = true;
            for i in &pending_behaviours {
                let behaviour = &mut all_behaviours[*i];
                let mut events = vec![];

                while let Poll::Ready(event) = behaviour.poll(&mut cx) {
                    events.push(event);
                }

                // If there are events, set completed to false to continue polling.
                if !events.is_empty() {
                    completed = false;

                    for event in events {
                        // Intercept the events that should be processed by the handler.
                        if let ToSwarm::NotifyHandler {
                            peer_id,
                            handler: NotifyHandler::One(connection_id),
                            event:
                                Either::Left(BehaviourEventToHandler::OutgoingMessage { message }),
                        } = event
                        {
                            // Deliver the message to the appropriate peer's handler.
                            deliver_message_to_peer(
                                &mut all_behaviours,
                                peer_id,
                                connection_id,
                                message.clone(),
                            );
                        }
                    }
                }
            }

            // Filter out behaviors that no longer generate events.
            pending_behaviours.retain(|i| {
                let mut events = vec![];
                let behaviour = &mut all_behaviours[*i];
                while let Poll::Ready(event) = behaviour.poll(&mut cx) {
                    events.push(event);
                }
                !events.is_empty()
            });
        }

        // Verify that all peers in subnet 0 have received the message, and others have not.
        let (subnet_0_behaviours, other_behaviours): (Vec<_>, Vec<_>) =
            all_behaviours.iter().partition(|behaviour| {
                behaviour
                    .membership
                    .membership(&behaviour.local_peer_id)
                    .contains(&0)
            });

        // Assert that all members of subnet 0 have received the message.
        for behaviour in &subnet_0_behaviours {
            assert!(behaviour
                .seen_message_cache
                .contains(&([0; 32].to_vec(), message.subnetwork_id)));
        }

        // Assert that no members of other subnets have received the message.
        for behaviour in &other_behaviours {
            assert!(behaviour.seen_message_cache.is_empty());
        }

        // Ensure the number of peers with the message matches the expected count
        assert_eq!(subnet_0_behaviours.len(), num_instances / 2);
    }
}

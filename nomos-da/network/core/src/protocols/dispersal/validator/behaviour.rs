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
        let Either::Left(event) = event;
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
    use crate::address_book::AddressBook;
    use crate::protocols::dispersal::executor::behaviour::{
        DispersalExecutorBehaviour, DispersalExecutorEvent,
    };
    use futures::task::ArcWake;
    use kzgrs_backend::common::blob::DaBlob;
    use kzgrs_backend::common::Column;
    use libp2p::identity::Keypair;
    use libp2p::swarm::SwarmEvent;
    use libp2p::{identity, quic, PeerId};
    use nomos_da_messages::common::Blob;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time;
    use tracing::{error, info};
    use tracing_subscriber::fmt::TestWriter;
    use tracing_subscriber::EnvFilter;

    #[derive(Clone, Debug)]
    struct Neighbourhood {
        pub membership: HashMap<PeerId, HashSet<SubnetworkId>>,
    }

    impl MembershipHandler for Neighbourhood {
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

    pub fn executor_swarm(
        addressbook: AddressBook,
        key: Keypair,
        peer_id: PeerId,
        membership: impl MembershipHandler<NetworkId = u32, Id = PeerId> + 'static,
    ) -> libp2p::Swarm<
        DispersalExecutorBehaviour<impl MembershipHandler<NetworkId = u32, Id = PeerId>>,
    > {
        info!("Creating executor_swarm with peerID {}", peer_id);

        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|keypair| quic::tokio::Transport::new(quic::Config::new(keypair)))
            .unwrap()
            .with_behaviour(|_key| {
                DispersalExecutorBehaviour::new(peer_id, membership, addressbook)
            })
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
            })
            .build()
    }

    pub fn validator_swarm(
        key: Keypair,
        membership: impl MembershipHandler<NetworkId = u32, Id = PeerId> + 'static,
    ) -> libp2p::Swarm<
        DispersalValidatorBehaviour<impl MembershipHandler<NetworkId = u32, Id = PeerId>>,
    > {
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|keypair| quic::tokio::Transport::new(quic::Config::new(keypair)))
            .unwrap()
            .with_behaviour(|_key| DispersalValidatorBehaviour::new(membership))
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(std::time::Duration::from_secs(u64::MAX))
            })
            .build()
    }

    fn prepare_swarm_config(num_instances: usize) -> Vec<(Keypair, PeerId, Multiaddr)> {
        let mut configs = Vec::with_capacity(num_instances);

        for i in 0..num_instances {
            let keypair = identity::Keypair::generate_ed25519();
            let peer_id = PeerId::from(keypair.public());
            let port = 5100 + i;
            let addr: Multiaddr = format!("/ip4/127.0.0.1/udp/{port}/quic-v1")
                .parse()
                .unwrap();

            configs.push((keypair, peer_id, addr));
        }
        configs
    }

    fn create_membership(
        num_instances: usize,
        subnet_id: u32,
        peer_ids: &[PeerId],
    ) -> Neighbourhood {
        let mut membership = HashMap::default();

        for i in 0..num_instances {
            membership.insert(peer_ids[i], HashSet::from([subnet_id]));
        }

        Neighbourhood { membership }
    }

    #[test]
    fn test_handle_established_inbound_connection() {
        let mut allowed_peers = HashMap::new();
        allowed_peers.insert(PeerId::random(), HashSet::from([0, 1]));
        let membership = Neighbourhood {
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
        let membership = Neighbourhood {
            membership: HashMap::new(),
        };
        let mut behaviour = DispersalValidatorBehaviour::new(membership);

        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        let poll_result = behaviour.poll(&mut cx);

        assert!(matches!(poll_result, Poll::Pending));
    }

    #[tokio::test]
    async fn test_validation_behaviour() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .compact()
            .with_writer(TestWriter::default())
            .try_init();

        let num_instances = 2;

        let subnet_0_config = prepare_swarm_config(num_instances / 2);
        let subnet_0_ids = subnet_0_config
            .iter()
            .map(|(_, peer_id, _)| peer_id.clone())
            .collect::<Vec<_>>();

        let subnet_1_config = prepare_swarm_config(num_instances / 2);
        let subnet_1_ids = subnet_1_config
            .iter()
            .map(|(_, peer_id, _)| peer_id.clone())
            .collect::<Vec<_>>();

        let addressbook =
            AddressBook::from_iter(subnet_1_config.iter().map(|(_, peer_id, addr)| {
                (
                    peer_id.clone(),
                    addr.clone().with_p2p(peer_id.clone()).unwrap(),
                )
            }));

        let subnet_0_membership = create_membership(num_instances / 2, 0, &subnet_0_ids);
        let subnet_1_membership = create_membership(num_instances / 2, 0, &subnet_1_ids);

        let mut all_neighbours = subnet_0_membership;
        all_neighbours
            .membership
            .extend(subnet_1_membership.membership);

        // create swarms
        let mut executor_swarms: Vec<_> = vec![];
        let mut validator_swarms: Vec<_> = vec![];

        for i in 0..num_instances / 2 {
            let (k, executor_peer, addr) = subnet_0_config[i].clone();
            let (k2, validator_peer, addr2) = subnet_1_config[i].clone();
            let executor = executor_swarm(
                addressbook.clone(),
                k,
                executor_peer,
                all_neighbours.clone(),
            );
            let validator = validator_swarm(k2, all_neighbours.clone());
            executor_swarms.push(executor);
            validator_swarms.push(validator);
        }

        let (validator_key, validator_id, validator_addr) = subnet_1_config[0].clone();
        let validator_addr_p2p = validator_addr
            .clone()
            .with_p2p(validator_id.clone())
            .unwrap();

        validator_swarms[0].listen_on(validator_addr).unwrap();

        let validator_behaviour = validator_swarms[0].behaviour_mut();
        let executor_disperse_blob_sender = executor_swarms[0].behaviour().blobs_sender();

        let msg_count = 10usize;
        let executor_poll = async move {
            let mut res = vec![];
            loop {
                tokio::select! {
                    Some(event) = executor_swarms[0].next() => {
                        info!("Executor event: {event:?}");
                        if let SwarmEvent::Behaviour(DispersalExecutorEvent::DispersalSuccess{blob_id, ..}) = event {
                            res.push(blob_id);
                        }
                    }

                    _ = time::sleep(Duration::from_secs(2)) => {
                        if res.len() < msg_count {error!("Executor timeout reached");}
                        break;
                    }
                }
            }
            res
        };

        let executor_task = tokio::spawn(executor_poll);

        for i in 0..msg_count {
            info!("Sending blob {i}...");
            executor_disperse_blob_sender
                .send((
                    0,
                    DaBlob {
                        column_idx: 0,
                        column: Column(vec![]),
                        column_commitment: Default::default(),
                        aggregated_column_commitment: Default::default(),
                        aggregated_column_proof: Default::default(),
                        rows_commitments: vec![],
                        rows_proofs: vec![],
                    },
                ))
                .unwrap();
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        let mut msg_counter = 0;
        loop {
            match validator_swarms[0].next().await {
                Some(SwarmEvent::Behaviour(DispersalEvent::IncomingMessage { message })) => {
                    info!("Validator received blob: {message:?}");

                    // Check data has structure and content as expected
                    match message.blob {
                        Some(Blob { blob_id, data }) => {
                            let deserialized_blob: DaBlob = bincode::deserialize(&data).unwrap();
                            assert_eq!(blob_id, deserialized_blob.id());
                            msg_counter += 1;
                        }
                        None => {}
                    }
                    match message.subnetwork_id {
                        subnet_id => {
                            assert_eq!(subnet_id, 0);
                        }
                    }
                }
                Some(event) => {
                    info!("Validator received event: {event:?}");
                }

                _ => {}
            }
            if msg_counter == msg_count {
                tokio::time::sleep(Duration::from_secs(2)).await;
                break;
            }
        }

        executor_task.await.unwrap();
    }
}

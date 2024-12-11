use std::io;
// std
use std::time::Duration;
// crates
use futures::StreamExt;
use kzgrs_backend::common::blob::DaBlob;
use libp2p::core::transport::ListenerId;
use libp2p::identity::Keypair;
use libp2p::swarm::{DialError, SwarmEvent};
use libp2p::{Multiaddr, PeerId, Swarm, SwarmBuilder, TransportError};
use log::debug;
use nomos_core::da::BlobId;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
// internal
use crate::address_book::AddressBook;
use crate::behaviour::executor::{ExecutorBehaviour, ExecutorBehaviourEvent};
use crate::protocols::{
    dispersal::{
        executor::behaviour::DispersalExecutorEvent, validator::behaviour::DispersalEvent,
    },
    replication::behaviour::ReplicationEvent,
    sampling::behaviour::SamplingEvent,
};
use crate::swarm::common::{
    handle_replication_event, handle_sampling_event, handle_validator_dispersal_event,
};
use crate::swarm::validator::ValidatorEventsStream;
use crate::SubnetworkId;
use subnetworks_assignations::MembershipHandler;

// Metrics
const EVENT_SAMPLING: &str = "sampling";
const EVENT_DISPERSAL_EXECUTOR_DISPERSAL: &str = "dispersal_executor_event";
const EVENT_VALIDATOR_DISPERSAL: &str = "validator_dispersal";
const EVENT_REPLICATION: &str = "replication";

pub struct ExecutorEventsStream {
    pub validator_events_stream: ValidatorEventsStream,
    pub dispersal_events_receiver: UnboundedReceiverStream<DispersalExecutorEvent>,
}

pub struct ExecutorSwarm<
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
> {
    swarm: Swarm<ExecutorBehaviour<Membership>>,
    sampling_events_sender: UnboundedSender<SamplingEvent>,
    validation_events_sender: UnboundedSender<DaBlob>,
    dispersal_events_sender: UnboundedSender<DispersalExecutorEvent>,
}

impl<Membership> ExecutorSwarm<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + Clone + Send,
{
    pub fn new(
        key: Keypair,
        membership: Membership,
        addresses: AddressBook,
    ) -> (Self, ExecutorEventsStream) {
        let (sampling_events_sender, sampling_events_receiver) = unbounded_channel();
        let (validation_events_sender, validation_events_receiver) = unbounded_channel();
        let (dispersal_events_sender, dispersal_events_receiver) = unbounded_channel();
        let sampling_events_receiver = UnboundedReceiverStream::new(sampling_events_receiver);
        let validation_events_receiver = UnboundedReceiverStream::new(validation_events_receiver);
        let dispersal_events_receiver = UnboundedReceiverStream::new(dispersal_events_receiver);

        (
            Self {
                swarm: Self::build_swarm(key, membership, addresses),
                sampling_events_sender,
                validation_events_sender,
                dispersal_events_sender,
            },
            ExecutorEventsStream {
                validator_events_stream: ValidatorEventsStream {
                    sampling_events_receiver,
                    validation_events_receiver,
                },
                dispersal_events_receiver,
            },
        )
    }
    fn build_swarm(
        key: Keypair,
        membership: Membership,
        addresses: AddressBook,
    ) -> Swarm<ExecutorBehaviour<Membership>> {
        SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| ExecutorBehaviour::new(key, membership, addresses))
            .expect("Validator behaviour should build")
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
            })
            .build()
    }

    pub fn dial(&mut self, addr: Multiaddr) -> Result<(), DialError> {
        self.swarm.dial(addr)?;
        Ok(())
    }

    pub fn listen_on(
        &mut self,
        address: Multiaddr,
    ) -> Result<ListenerId, TransportError<io::Error>> {
        self.swarm.listen_on(address)
    }

    pub fn sample_request_channel(&mut self) -> UnboundedSender<(Membership::NetworkId, BlobId)> {
        self.swarm
            .behaviour()
            .sampling_behaviour()
            .sample_request_channel()
    }

    pub fn dispersal_blobs_channel(&mut self) -> UnboundedSender<(Membership::NetworkId, DaBlob)> {
        self.swarm
            .behaviour()
            .dispersal_executor_behaviour()
            .blobs_sender()
    }

    pub fn dispersal_open_stream_sender(&mut self) -> UnboundedSender<PeerId> {
        self.swarm
            .behaviour()
            .dispersal_executor_behaviour()
            .open_stream_sender()
    }

    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    pub fn protocol_swarm(&self) -> &Swarm<ExecutorBehaviour<Membership>> {
        &self.swarm
    }

    pub fn protocol_swarm_mut(&mut self) -> &mut Swarm<ExecutorBehaviour<Membership>> {
        &mut self.swarm
    }

    async fn handle_sampling_event(&mut self, event: SamplingEvent) {
        handle_sampling_event(&mut self.sampling_events_sender, event).await
    }

    async fn handle_executor_dispersal_event(&mut self, event: DispersalExecutorEvent) {
        if let Err(e) = self.dispersal_events_sender.send(event) {
            debug!("Error distributing sampling message internally: {e:?}");
        }
    }

    async fn handle_validator_dispersal_event(&mut self, event: DispersalEvent) {
        handle_validator_dispersal_event(
            &mut self.validation_events_sender,
            self.swarm.behaviour_mut().replication_behaviour_mut(),
            event,
        )
        .await
    }

    async fn handle_replication_event(&mut self, event: ReplicationEvent) {
        handle_replication_event(&mut self.validation_events_sender, event).await
    }

    async fn handle_behaviour_event(&mut self, event: ExecutorBehaviourEvent<Membership>) {
        match event {
            ExecutorBehaviourEvent::Sampling(event) => {
                tracing::info!(
                    counter.behaviour_events_received = 1,
                    event = EVENT_SAMPLING
                );
                self.handle_sampling_event(event).await;
            }
            ExecutorBehaviourEvent::ExecutorDispersal(event) => {
                tracing::info!(
                    counter.behaviour_events_received = 1,
                    event = EVENT_DISPERSAL_EXECUTOR_DISPERSAL
                );
                self.handle_executor_dispersal_event(event).await;
            }
            ExecutorBehaviourEvent::ValidatorDispersal(event) => {
                tracing::info!(
                    counter.behaviour_events_received = 1,
                    event = EVENT_VALIDATOR_DISPERSAL,
                    blob_size = event.blob_size()
                );
                self.handle_validator_dispersal_event(event).await;
            }
            ExecutorBehaviourEvent::Replication(event) => {
                tracing::info!(
                    counter.behaviour_events_received = 1,
                    event = EVENT_REPLICATION,
                    blob_size = event.blob_size()
                );
                self.handle_replication_event(event).await;
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            if let Some(event) = self.swarm.next().await {
                tracing::info!("Da swarm event received: {event:?}");
                match event {
                    SwarmEvent::Behaviour(behaviour_event) => {
                        self.handle_behaviour_event(behaviour_event).await;
                    }
                    SwarmEvent::ConnectionEstablished { .. } => {}
                    SwarmEvent::ConnectionClosed { .. } => {}
                    SwarmEvent::IncomingConnection { .. } => {}
                    SwarmEvent::IncomingConnectionError { .. } => {}
                    SwarmEvent::OutgoingConnectionError { .. } => {}
                    SwarmEvent::NewListenAddr { .. } => {}
                    SwarmEvent::ExpiredListenAddr { .. } => {}
                    SwarmEvent::ListenerClosed { .. } => {}
                    SwarmEvent::ListenerError { .. } => {}
                    SwarmEvent::Dialing { .. } => {}
                    SwarmEvent::NewExternalAddrCandidate { .. } => {}
                    SwarmEvent::ExternalAddrConfirmed { .. } => {}
                    SwarmEvent::ExternalAddrExpired { .. } => {}
                    SwarmEvent::NewExternalAddrOfPeer { .. } => {}
                    event => {
                        debug!("Unsupported validator swarm event: {event:?}");
                    }
                }
            }
        }
    }
}

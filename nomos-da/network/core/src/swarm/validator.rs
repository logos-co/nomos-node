// std
// crates
use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm, SwarmBuilder};
use log::debug;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
// internal
use crate::behaviour::validator::{ValidatorBehaviour, ValidatorBehaviourEvent};
use crate::protocols::{
    dispersal::validator::behaviour::DispersalEvent, replication::behaviour::ReplicationEvent,
    sampling::behaviour::SamplingEvent,
};
use crate::SubnetworkId;
use subnetworks_assignations::MembershipHandler;

pub struct ValidatorEventsStream {
    pub sampling_events_receiver: UnboundedReceiverStream<SamplingEvent>,
}

pub struct ValidatorSwarm<
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
> {
    swarm: Swarm<ValidatorBehaviour<Membership>>,
    sampling_events_sender: UnboundedSender<SamplingEvent>,
}

impl<Membership> ValidatorSwarm<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + Clone + Send,
{
    pub fn new(key: Keypair, membership: Membership) -> (Self, ValidatorEventsStream) {
        let (sampling_events_sender, sampling_events_receiver) = unbounded_channel();
        let sampling_events_receiver = UnboundedReceiverStream::new(sampling_events_receiver);
        (
            Self {
                swarm: Self::build_swarm(key, membership),
                sampling_events_sender,
            },
            ValidatorEventsStream {
                sampling_events_receiver,
            },
        )
    }
    fn build_swarm(key: Keypair, membership: Membership) -> Swarm<ValidatorBehaviour<Membership>> {
        SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| ValidatorBehaviour::new(key, membership))
            .expect("Validator behaviour should build")
            .build()
    }

    pub fn protocol_swarm(&self) -> &Swarm<ValidatorBehaviour<Membership>> {
        &self.swarm
    }

    pub fn protocol_swarm_mut(&mut self) -> &mut Swarm<ValidatorBehaviour<Membership>> {
        &mut self.swarm
    }

    async fn handle_sampling_event(&mut self, event: SamplingEvent) {
        if let Err(e) = self.sampling_events_sender.send(event) {
            debug!("Error distributing sampling message internally: {e:?}");
        }
    }

    async fn handle_dispersal_event(&mut self, _event: DispersalEvent) {
        // TODO: hook incoming dispersal events => to replication
        unimplemented!()
    }

    async fn handle_replication_event(&mut self, _event: ReplicationEvent) {
        // TODO: Hook incoming blobs from replication protocol
        unimplemented!()
    }

    async fn handle_behaviour_event(&mut self, event: ValidatorBehaviourEvent<Membership>) {
        match event {
            ValidatorBehaviourEvent::Sampling(event) => {
                self.handle_sampling_event(event).await;
            }
            ValidatorBehaviourEvent::Dispersal(event) => {
                self.handle_dispersal_event(event).await;
            }
            ValidatorBehaviourEvent::Replication(event) => {
                self.handle_replication_event(event).await;
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            if let Some(event) = self.swarm.next().await {
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

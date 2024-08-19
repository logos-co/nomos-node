use crate::behaviour::validator::{ValidatorBehaviour, ValidatorBehaviourEvent};
use crate::dispersal::validator::behaviour::DispersalEvent;
use crate::replication::behaviour::ReplicationEvent;
use crate::sampling::behaviour::SamplingEvent;
use crate::SubnetworkId;
use futures::StreamExt;
use libp2p::identity::Keypair;
use libp2p::swarm::SwarmEvent;
use libp2p::{PeerId, Swarm, SwarmBuilder};
use log::debug;
use subnetworks_assignations::MembershipHandler;

pub struct ValidatorSwarm<
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + 'static,
> {
    swarm: Swarm<ValidatorBehaviour<Membership>>,
}

impl<Membership> ValidatorSwarm<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + Clone + Send,
{
    pub fn build_swarm(
        key: Keypair,
        membership: Membership,
    ) -> Swarm<ValidatorBehaviour<Membership>> {
        SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| ValidatorBehaviour::new(key, membership))
            .expect("Validator behaviour should build")
            .build()
    }

    async fn handle_sampling_event(&mut self, event: SamplingEvent) {
        unimplemented!()
    }

    async fn handle_dispersal_event(&mut self, event: DispersalEvent) {
        unimplemented!()
    }

    async fn handle_replication_event(&mut self, event: ReplicationEvent) {
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

    pub async fn step(&mut self) {
        tokio::select! {
            Some(event) = self.swarm.next() => {
                match event {
                    SwarmEvent::Behaviour(behaviour_event) => {
                        self.handle_behaviour_event(behaviour_event).await;
                    },
                    SwarmEvent::ConnectionEstablished{ .. } => {}
                    SwarmEvent::ConnectionClosed{ .. } => {}
                    SwarmEvent::IncomingConnection{ .. } => {}
                    SwarmEvent::IncomingConnectionError{ .. } => {}
                    SwarmEvent::OutgoingConnectionError{ .. } => {}
                    SwarmEvent::NewListenAddr{ .. } => {}
                    SwarmEvent::ExpiredListenAddr{ .. } => {}
                    SwarmEvent::ListenerClosed{ .. } => {}
                    SwarmEvent::ListenerError{ .. } => {}
                    SwarmEvent::Dialing{ .. } => {}
                    SwarmEvent::NewExternalAddrCandidate{ .. } => {}
                    SwarmEvent::ExternalAddrConfirmed{ .. } => {}
                    SwarmEvent::ExternalAddrExpired{ .. } => {}
                    SwarmEvent::NewExternalAddrOfPeer{ .. } => {}
                    event => {
                        debug!("Unsupported validator swarm event: {event:?}");
                    }
                }
            }

        }
    }
}

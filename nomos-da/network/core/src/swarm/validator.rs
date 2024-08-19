use crate::behaviour::validator::{ValidatorBehaviour, ValidatorBehaviourEvent};
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

    async fn handle_behaviour_event(&mut self, event: ValidatorBehaviourEvent<Membership>) {
        match event {
            ValidatorBehaviourEvent::Sampling(_) => {
                unimplemented!()
            }
            ValidatorBehaviourEvent::Dispersal(_) => {
                unimplemented!()
            }
            ValidatorBehaviourEvent::Replication(_) => {
                unimplemented!()
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

use crate::behaviour::ValidatorBehaviour;
use crate::SubnetworkId;
use libp2p::identity::Keypair;
use libp2p::{PeerId, Swarm, SwarmBuilder};
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
    fn build_swarm(key: Keypair, membership: Membership) -> Swarm<ValidatorBehaviour<Membership>> {
        SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| ValidatorBehaviour::new(key, membership))
            .expect("Validator behaviour should build")
            .build()
    }
}

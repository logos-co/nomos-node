// std

use libp2p::identity::Keypair;
use libp2p::PeerId;
// crates
use libp2p::swarm::NetworkBehaviour;
// internal
use crate::{
    dispersal::validator::behaviour::DispersalValidatorBehaviour,
    replication::behaviour::ReplicationBehaviour, sampling::behaviour::SamplingBehaviour,
};
use subnetworks_assignations::MembershipHandler;

/// Aggregated `NetworkBehaviour` composed of:
/// * Sampling
/// * Dispersal
/// * Replication
///     WARNING: Order of internal protocols matters as the first one will be polled first until return
///     a `Poll::Pending`.
/// 1) Sampling is the crucial one as we have to be responsive for consensus.
/// 2) Dispersal so we do not bottleneck executors.
/// 3) Replication is the least important (and probably the least used), it is also dependant of dispersal.
#[derive(NetworkBehaviour)]
pub struct ValidatorBehaviour<Membership: MembershipHandler> {
    sampling: SamplingBehaviour<Membership>,
    dispersal: DispersalValidatorBehaviour<Membership>,
    replication: ReplicationBehaviour<Membership>,
}

impl<Membership> ValidatorBehaviour<Membership>
where
    Membership: MembershipHandler + Clone + Send + 'static,
    <Membership as MembershipHandler>::NetworkId: Send,
{
    pub fn new(key: &Keypair, membership: Membership) -> Self {
        let peer_id = PeerId::from_public_key(&key.public());
        Self {
            sampling: SamplingBehaviour::new(peer_id, membership.clone()),
            dispersal: DispersalValidatorBehaviour::new(membership.clone()),
            replication: ReplicationBehaviour::new(peer_id, membership),
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        // TODO: share membership
        self.sampling.update_membership(membership.clone());
        self.dispersal.update_membership(membership.clone());
        self.replication.update_membership(membership);
    }
}

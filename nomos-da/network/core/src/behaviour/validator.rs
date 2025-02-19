// std

use std::time::Duration;

use libp2p::identity::Keypair;
use libp2p::PeerId;
// crates
use libp2p::swarm::NetworkBehaviour;
// internal
use crate::address_book::AddressBook;
use crate::maintenance::monitor::{ConnectionMonitor, ConnectionMonitorBehaviour};
use crate::{
    protocols::dispersal::validator::behaviour::DispersalValidatorBehaviour,
    protocols::replication::behaviour::ReplicationBehaviour,
    protocols::sampling::behaviour::SamplingBehaviour,
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
pub struct ValidatorBehaviour<Monitor, Membership>
where
    Monitor: ConnectionMonitor,
    Membership: MembershipHandler,
{
    sampling: SamplingBehaviour<Membership>,
    dispersal: DispersalValidatorBehaviour<Membership>,
    replication: ReplicationBehaviour<Membership>,
    monitor: ConnectionMonitorBehaviour<Monitor>,
}

impl<Monitor, Membership> ValidatorBehaviour<Monitor, Membership>
where
    Monitor: ConnectionMonitor,
    Membership: MembershipHandler + Clone + Send + 'static,
    <Membership as MembershipHandler>::NetworkId: Send,
{
    pub fn new(
        key: &Keypair,
        membership: Membership,
        addresses: AddressBook,
        monitor: Monitor,
        redial_cooldown: Duration,
    ) -> Self {
        let peer_id = PeerId::from_public_key(&key.public());
        Self {
            sampling: SamplingBehaviour::new(peer_id, membership.clone(), addresses),
            dispersal: DispersalValidatorBehaviour::new(membership.clone()),
            replication: ReplicationBehaviour::new(peer_id, membership),
            monitor: ConnectionMonitorBehaviour::new(monitor, redial_cooldown),
        }
    }

    pub fn update_membership(&mut self, membership: Membership) {
        // TODO: share membership
        self.sampling.update_membership(membership.clone());
        self.dispersal.update_membership(membership.clone());
        self.replication.update_membership(membership);
    }

    pub fn sampling_behaviour(&self) -> &SamplingBehaviour<Membership> {
        &self.sampling
    }

    pub fn dispersal_behaviour(&self) -> &DispersalValidatorBehaviour<Membership> {
        &self.dispersal
    }

    pub fn replication_behaviour(&self) -> &ReplicationBehaviour<Membership> {
        &self.replication
    }

    pub fn sampling_behaviour_mut(&mut self) -> &mut SamplingBehaviour<Membership> {
        &mut self.sampling
    }

    pub fn dispersal_behaviour_mut(&mut self) -> &mut DispersalValidatorBehaviour<Membership> {
        &mut self.dispersal
    }

    pub fn replication_behaviour_mut(&mut self) -> &mut ReplicationBehaviour<Membership> {
        &mut self.replication
    }

    pub fn monitor_behaviour_mut(&mut self) -> &mut ConnectionMonitorBehaviour<Monitor> {
        &mut self.monitor
    }
}

use std::collections::HashSet;

use libp2p::core::{transport::MemoryTransport, upgrade::Version};
use libp2p::PeerId;
use libp2p::Transport;
use libp2p::{identity::Keypair, swarm::NetworkBehaviour};
use std::time::Duration;
use subnetworks_assignations::MembershipHandler;

use crate::SubnetworkId;

#[derive(Clone)]
pub struct AllNeighbours {
    pub neighbours: HashSet<PeerId>,
}

impl MembershipHandler for AllNeighbours {
    type NetworkId = SubnetworkId;
    type Id = PeerId;

    fn membership(&self, _self_id: &Self::Id) -> HashSet<Self::NetworkId> {
        [0].into_iter().collect()
    }

    fn is_allowed(&self, _id: &Self::Id) -> bool {
        true
    }

    fn members_of(&self, _network_id: &Self::NetworkId) -> HashSet<Self::Id> {
        self.neighbours.clone()
    }

    fn members(&self) -> HashSet<Self::Id> {
        self.neighbours.clone()
    }

    fn last_subnetwork_id(&self) -> Self::NetworkId {
        unimplemented!()
    }
}

pub fn new_swarm_in_memory<TBehavior>(key: Keypair, behavior: TBehavior) -> libp2p::Swarm<TBehavior>
where
    TBehavior: NetworkBehaviour + Send,
{
    libp2p::SwarmBuilder::with_existing_identity(key.clone())
        .with_tokio()
        .with_other_transport(|_| {
            let transport = MemoryTransport::default()
                .upgrade(Version::V1)
                .authenticate(libp2p::plaintext::Config::new(&key))
                .multiplex(libp2p::yamux::Config::default())
                .timeout(Duration::from_secs(20));

            Ok(transport)
        })
        .unwrap()
        .with_behaviour(|_| behavior)
        .unwrap()
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX)))
        .build()
}

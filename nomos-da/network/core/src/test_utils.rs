use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

use libp2p::{
    core::{transport::MemoryTransport, upgrade::Version},
    identity::Keypair,
    swarm::NetworkBehaviour,
    PeerId, Transport,
};
use subnetworks_assignations::MembershipHandler;

use crate::SubnetworkId;

#[derive(Clone)]
pub struct AllNeighbours {
    pub neighbours: Arc<Mutex<HashSet<PeerId>>>,
}

impl MembershipHandler for AllNeighbours {
    type NetworkId = SubnetworkId;
    type Id = PeerId;

    fn membership(&self, _self_id: &Self::Id) -> HashSet<Self::NetworkId> {
        std::iter::once(0).collect()
    }

    fn is_allowed(&self, _id: &Self::Id) -> bool {
        true
    }

    fn members_of(&self, _network_id: &Self::NetworkId) -> HashSet<Self::Id> {
        self.neighbours.lock().unwrap().clone()
    }

    fn members(&self) -> HashSet<Self::Id> {
        self.neighbours.lock().unwrap().clone()
    }

    fn last_subnetwork_id(&self) -> Self::NetworkId {
        0
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

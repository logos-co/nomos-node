// std
// crates
use kzgrs_backend::common::blob::DaBlob;
use libp2p::futures::StreamExt;
use libp2p::Multiaddr;
use libp2p::{identity::Keypair, swarm::SwarmEvent, PeerId, Swarm};
use nomos_core::da::BlobId;
use nomos_da_network_core::protocols::dispersal::executor::behaviour::{
    DispersalError, DispersalExecutorEvent,
};
use nomos_da_network_core::{
    protocols::dispersal::executor::behaviour::DispersalExecutorBehaviour, SubnetworkId,
};
use nomos_libp2p::DialError;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error};
// internal

#[derive(Debug, Clone)]
pub enum DispersalEvent {
    /// A blob successfully arrived its destination
    DispersalSuccess {
        blob_id: BlobId,
        subnetwork_id: SubnetworkId,
    },
    /// Something went wrong delivering the blob
    DispersalError { error: DispersalError },
}

pub struct ExecutorSwarm<Membership>
where
    Membership: MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static,
{
    swarm: Swarm<DispersalExecutorBehaviour<Membership>>,
    dispersal_broadcast_sender: UnboundedSender<DispersalEvent>,
}

impl<Membership> ExecutorSwarm<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId> + Clone + Send,
{
    pub fn new(
        key: Keypair,
        membership: Membership,
        dispersal_broadcast_sender: UnboundedSender<DispersalEvent>,
    ) -> Self {
        let swarm = Self::build_swarm(key, membership);
        Self {
            swarm,
            dispersal_broadcast_sender,
        }
    }

    pub fn blobs_sender(&self) -> UnboundedSender<(SubnetworkId, DaBlob)> {
        self.swarm.behaviour().blobs_sender()
    }

    fn build_swarm(
        key: Keypair,
        membership: Membership,
    ) -> Swarm<DispersalExecutorBehaviour<Membership>> {
        libp2p::SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_quic()
            .with_behaviour(|_key| DispersalExecutorBehaviour::new(membership))
            .expect("Validator behaviour should build")
            .build()
    }

    pub fn dial(&mut self, addr: Multiaddr) -> Result<(), DialError> {
        self.swarm.dial(addr)?;
        Ok(())
    }

    pub async fn run(&mut self) {
        tokio::select! {
            Some(event) = self.swarm.next() => {
                match event {
                    SwarmEvent::Behaviour(behaviour_event) => {
                        self.handle_dispersal_event(behaviour_event).await;
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

    async fn handle_dispersal_event(&mut self, event: DispersalExecutorEvent) {
        match event {
            DispersalExecutorEvent::DispersalSuccess {
                blob_id,
                subnetwork_id,
            } => {
                if let Err(e) =
                    self.dispersal_broadcast_sender
                        .send(DispersalEvent::DispersalSuccess {
                            blob_id,
                            subnetwork_id,
                        })
                {
                    error!("Error in internal broadcast of dispersal success: {e:?}");
                }
            }
            DispersalExecutorEvent::DispersalError { error } => {
                if let Err(e) = self
                    .dispersal_broadcast_sender
                    .send(DispersalEvent::DispersalError { error })
                {
                    error! {"Error in internal broadcast of dispersal error: {e:?}"};
                }
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use crate::da::network::swarm::ExecutorSwarm;
    use crate::test_utils::AllNeighbours;
    use kzgrs_backend::common::blob::DaBlob;
    use kzgrs_backend::common::Column;
    use libp2p::identity::Keypair;
    use libp2p::PeerId;
    use nomos_da_network_core::address_book::AddressBook;
    use nomos_da_network_core::swarm::validator::ValidatorSwarm;
    use nomos_libp2p::Multiaddr;
    use std::time::Duration;
    use tokio::sync::mpsc::unbounded_channel;

    #[tokio::test]
    async fn test_dispersal_with_swarms() {
        let k1 = Keypair::generate_ed25519();
        let k2 = Keypair::generate_ed25519();
        let executor_peer = PeerId::from_public_key(&k1.public());
        let validator_peer = PeerId::from_public_key(&k2.public());
        let neighbours = AllNeighbours {
            neighbours: [
                PeerId::from_public_key(&k1.public()),
                PeerId::from_public_key(&k2.public()),
            ]
            .into_iter()
            .collect(),
        };

        let addr: Multiaddr = "/ip4/127.0.0.1/udp/5063/quic-v1".parse().unwrap();
        let addr2 = addr.clone().with_p2p(validator_peer).unwrap();
        let addr2_book = AddressBook::from_iter(vec![(executor_peer, addr2.clone())]);

        let (dispersal_broadcast_sender, _) = unbounded_channel();

        let mut executor =
            ExecutorSwarm::new(k1, neighbours.clone(), dispersal_broadcast_sender.clone());
        let (mut validator, _) =
            ValidatorSwarm::new(k2, neighbours.clone(), addr2_book);

        tokio::spawn(async move {
            let validator_swarm = validator.protocol_swarm_mut();
            validator_swarm.listen_on(addr).unwrap();

            validator.run().await;
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        executor.dial(addr2).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;

        let executor_disperse_blob_sender = executor.swarm.behaviour().blobs_sender();

        println!("Sending blob...");
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

        let executor_task = tokio::spawn(async move {executor.run().await;});
        assert!(executor_task.await.is_ok());
    }
}

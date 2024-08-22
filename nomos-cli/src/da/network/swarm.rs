use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

// std
// crates
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::encoder::EncodedData as KzgEncodedData;
use libp2p::futures::StreamExt;
use libp2p::Multiaddr;
use libp2p::{identity::Keypair, swarm::SwarmEvent, PeerId, Swarm};
use nomos_core::da::{BlobId, DaDispersal};
use nomos_da_network_core::protocols::dispersal::executor::behaviour::{
    DispersalError, DispersalExecutorEvent,
};
use nomos_da_network_core::{
    protocols::dispersal::executor::behaviour::DispersalExecutorBehaviour, SubnetworkId,
};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error};
// internal

#[derive(Debug)]
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
    open_stream_sender: UnboundedSender<PeerId>,
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
        let open_stream_sender = swarm.behaviour().open_stream_sender();
        Self {
            swarm,
            open_stream_sender,
            dispersal_broadcast_sender,
        }
    }

    pub fn blobs_sender(&self) -> UnboundedSender<(u32, DaBlob)> {
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

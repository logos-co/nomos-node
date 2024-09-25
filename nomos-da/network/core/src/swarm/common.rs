use crate::protocols::dispersal::validator::behaviour::DispersalEvent;
use crate::protocols::replication::behaviour::{ReplicationBehaviour, ReplicationEvent};
use crate::protocols::sampling::behaviour::SamplingEvent;
use crate::SubnetworkId;
use kzgrs_backend::common::blob::DaBlob;
use libp2p::PeerId;
use log::{debug, error};
use nomos_da_messages::replication::ReplicationReq;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc::UnboundedSender;

pub async fn handle_validator_dispersal_event<Membership>(
    validation_events_sender: &mut UnboundedSender<DaBlob>,
    replication_behaviour: &mut ReplicationBehaviour<Membership>,
    event: DispersalEvent,
) where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    match event {
        // Send message for replication
        DispersalEvent::IncomingMessage { message } => {
            if let Ok(blob) = bincode::deserialize::<DaBlob>(
                message
                    .blob
                    .as_ref()
                    .expect("Message blob should not be empty")
                    .data
                    .as_slice(),
            ) {
                if let Err(e) = validation_events_sender.send(blob) {
                    error!("Error sending blob to validation: {e:?}");
                }
            }
            replication_behaviour.send_message(ReplicationReq {
                blob: message.blob,
                subnetwork_id: message.subnetwork_id,
            });
        }
    }
}

pub async fn handle_sampling_event<Membership>(
    sampling_events_sender: &mut UnboundedSender<SamplingEvent>,
    event: SamplingEvent,
) where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    if let Err(e) = sampling_events_sender.send(event) {
        debug!("Error distributing sampling message internally: {e:?}");
    }
}

pub async fn handle_replication_event(
    validation_events_sender: &mut UnboundedSender<DaBlob>,
    event: ReplicationEvent,
) {
    let ReplicationEvent::IncomingMessage { message, .. } = event;
    if let Ok(blob) = bincode::deserialize::<DaBlob>(
        message
            .blob
            .as_ref()
            .expect("Message blob should not be empty")
            .data
            .as_slice(),
    ) {
        if let Err(e) = validation_events_sender.send(blob) {
            error!("Error sending blob to validation: {e:?}");
        }
    }
}

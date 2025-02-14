use crate::protocols::dispersal::validator::behaviour::DispersalEvent;
use crate::protocols::replication::behaviour::{ReplicationBehaviour, ReplicationEvent};
use crate::protocols::sampling::behaviour::SamplingEvent;
use crate::SubnetworkId;
use kzgrs_backend::common::blob::DaBlob;
use libp2p::PeerId;
use log::{debug, error};
use nomos_da_messages::replication;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc::UnboundedSender;

pub async fn handle_validator_dispersal_event<Membership>(
    validation_events_sender: &mut UnboundedSender<DaBlob>,
    replication_behaviour: &mut ReplicationBehaviour<Membership>,
    event: DispersalEvent,
) where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>,
{
    // Send message for replication
    if let DispersalEvent::IncomingMessage { message } = event {
        let blob_message = message.blob;
        if let Err(e) = validation_events_sender.send(blob_message.data.clone()) {
            error!("Error sending blob to validation: {e:?}");
        }
        replication_behaviour.send_message(replication::ReplicationRequest::new(
            blob_message,
            message.subnetwork_id,
        ));
    }
}

pub async fn handle_sampling_event(
    sampling_events_sender: &mut UnboundedSender<SamplingEvent>,
    event: SamplingEvent,
) {
    if let Err(e) = sampling_events_sender.send(event) {
        debug!("Error distributing sampling message internally: {e:?}");
    }
}

pub async fn handle_replication_event(
    validation_events_sender: &mut UnboundedSender<DaBlob>,
    event: ReplicationEvent,
) {
    if let ReplicationEvent::IncomingMessage { message, .. } = event {
        if let Err(e) = validation_events_sender.send(message.blob.data) {
            error!("Error sending blob to validation: {e:?}");
        }
    }
}

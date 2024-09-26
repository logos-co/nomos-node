use futures::StreamExt;
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::common::ColumnIndex;
use libp2p::swarm::NetworkBehaviour;
use libp2p::Swarm;
use log::error;
use nomos_core::da::BlobId;
use nomos_da_network_core::protocols::sampling;
use nomos_da_network_core::protocols::sampling::behaviour::{
    BehaviourSampleReq, BehaviourSampleRes, SamplingError,
};
use nomos_da_network_core::swarm::validator::ValidatorEventsStream;
use nomos_da_network_core::SubnetworkId;
use nomos_libp2p::secret_key_serde;
use nomos_libp2p::{ed25519, Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Debug;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{broadcast, mpsc};

pub(crate) const BROADCAST_CHANNEL_SIZE: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaNetworkBackendSettings<Membership> {
    // Identification Secp256k1 private key in Hex format (`0x123...abc`). Default random.
    #[serde(with = "secret_key_serde", default = "ed25519::SecretKey::generate")]
    pub node_key: ed25519::SecretKey,
    /// Membership of DA network PoV set
    pub membership: Membership,
    pub addresses: Vec<(PeerId, Multiaddr)>,
    pub listening_address: Multiaddr,
}

/// Sampling events coming from da network
#[derive(Debug, Clone)]
pub enum SamplingEvent {
    /// A success sampling
    SamplingSuccess { blob_id: BlobId, blob: Box<DaBlob> },
    /// Incoming sampling request
    SamplingRequest {
        blob_id: BlobId,
        column_idx: ColumnIndex,
        response_sender: mpsc::Sender<Option<DaBlob>>,
    },
    /// A failed sampling error
    SamplingError { error: SamplingError },
}

pub(crate) fn dial_peers<Membership, Behaviour>(
    membership: Membership,
    addresses: Vec<(PeerId, Multiaddr)>,
    swarm: &mut Swarm<Behaviour>,
    local_peer_id: PeerId,
) where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Behaviour: NetworkBehaviour,
{
    let mut connected_peers = HashSet::new();
    membership
        .membership(&local_peer_id)
        .iter()
        .flat_map(|subnet| membership.members_of(subnet))
        .filter(|peer| peer != &local_peer_id)
        .filter_map(|peer| {
            addresses
                .iter()
                .find(|(p, _)| p == &peer)
                .map(|(_, addr)| (peer, addr.clone()))
        })
        .for_each(|(peer, addr)| {
            // Only dial if we haven't already connected to this peer.
            if connected_peers.insert(peer) {
                swarm
                    .dial(addr)
                    .expect("Node should be able to dial peer in a subnet");
            }
        });
}

/// Task that handles forwarding of events to the subscriptions channels/stream
pub(crate) async fn handle_validator_events_stream(
    events_streams: ValidatorEventsStream,
    sampling_broadcast_sender: broadcast::Sender<SamplingEvent>,
    validation_broadcast_sender: broadcast::Sender<DaBlob>,
) {
    let ValidatorEventsStream {
        mut sampling_events_receiver,
        mut validation_events_receiver,
    } = events_streams;
    #[allow(clippy::never_loop)]
    loop {
        // WARNING: `StreamExt::next` is cancellation safe.
        // If adding more branches check if such methods are within the cancellation safe set:
        // https://docs.rs/tokio/latest/tokio/macro.select.html#cancellation-safety
        tokio::select! {
            Some(sampling_event) = StreamExt::next(&mut sampling_events_receiver) => {
                match sampling_event {
                    sampling::behaviour::SamplingEvent::SamplingSuccess{ blob_id, blob , .. } => {
                        if let Err(e) = sampling_broadcast_sender.send(SamplingEvent::SamplingSuccess {blob_id, blob}){
                            error!("Error in internal broadcast of sampling success: {e:?}");
                        }
                    }
                    sampling::behaviour::SamplingEvent::IncomingSample{request_receiver, response_sender} => {
                        if let Ok(BehaviourSampleReq { blob_id, column_idx }) = request_receiver.await {
                            let (sampling_response_sender, mut sampling_response_receiver) = mpsc::channel(1);

                            if let Err(e) = sampling_broadcast_sender
                                .send(SamplingEvent::SamplingRequest { blob_id, column_idx, response_sender: sampling_response_sender })
                            {
                                error!("Error in internal broadcast of sampling request: {e:?}");
                                sampling_response_receiver.close()
                            }

                            if let Some(maybe_blob) = sampling_response_receiver.recv().await {
                                let result = match maybe_blob {
                                    Some(blob) => BehaviourSampleRes::SamplingSuccess {
                                        blob_id,
                                        subnetwork_id: blob.column_idx as u32,
                                        blob: Box::new(blob),
                                    },
                                    None => BehaviourSampleRes::SampleNotFound { blob_id },
                                };

                                if response_sender.send(result).is_err() {
                                    error!("Error sending sampling success response");
                                }
                            } else if response_sender
                                .send(BehaviourSampleRes::SampleNotFound { blob_id })
                                .is_err()
                            {
                                error!("Error sending sampling success response");
                            }
                        }
                    }
                    sampling::behaviour::SamplingEvent::SamplingError{ error  } => {
                        if let Err(e) = sampling_broadcast_sender.send(SamplingEvent::SamplingError {error}) {
                            error!{"Error in internal broadcast of sampling error: {e:?}"};
                        }
                    }}
            }
            Some(da_blob) = StreamExt::next(&mut validation_events_receiver)=> {
                if let Err(error) = validation_broadcast_sender.send(da_blob) {
                    error!("Error in internal broadcast of validation for blob: {:?}", error.0);
                }
            }
        }
    }
}

pub(crate) async fn handle_sample_request(
    sampling_request_channel: &UnboundedSender<(SubnetworkId, BlobId)>,
    subnetwork_id: SubnetworkId,
    blob_id: BlobId,
) {
    if let Err(SendError((subnetwork_id, blob_id))) =
        sampling_request_channel.send((subnetwork_id, blob_id))
    {
        error!("Error requesting sample for subnetwork id : {subnetwork_id}, blob_id: {blob_id:?}");
    }
}

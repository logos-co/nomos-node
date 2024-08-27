// std
use std::fmt::Debug;
use std::pin::Pin;

// crates
use futures::{Stream, StreamExt};
use libp2p_identity::PeerId;
use tokio::sync::oneshot;
// internal
use crate::network::NetworkAdapter;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::backends::libp2p::validator::{
    DaNetworkEvent, DaNetworkEventKind, DaNetworkMessage, DaNetworkValidatorBackend, SamplingEvent,
};
use nomos_da_network_service::{DaNetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;
use subnetworks_assignations::MembershipHandler;

#[derive(Debug, Clone)]
pub struct DaNetworkSamplingSettings {
    pub num_samples: u16,
    pub subnet_size: SubnetworkId,
}

pub struct Libp2pAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Debug
        + Clone
        + Send
        + Sync
        + 'static,
{
    settings: DaNetworkSamplingSettings,
    network_relay: OutboundRelay<
        <NetworkService<DaNetworkValidatorBackend<Membership>> as ServiceData>::Message,
    >,
}

#[async_trait::async_trait]
impl<Membership> NetworkAdapter for Libp2pAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Debug
        + Clone
        + Send
        + Sync
        + 'static,
{
    type Backend = DaNetworkValidatorBackend<Membership>;
    type Settings = DaNetworkSamplingSettings;

    async fn new(
        settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            settings,
            network_relay,
        }
    }

    async fn start_sampling(
        &mut self,
        blob_id: BlobId,
        subnets: &Vec<SubnetworkId>,
    ) -> Result<(), DynError> {
        for id in subnets.iter() {
            let subnetwork_id = id.clone();
            self.network_relay
                .send(DaNetworkMsg::Process(DaNetworkMessage::RequestSample {
                    blob_id,
                    subnetwork_id,
                }))
                .await
                .expect("RequestSample message should have been sent")
        }
        Ok(())
    }

    async fn listen_to_sampling_messages(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = SamplingEvent> + Send>>, DynError> {
        let (stream_sender, stream_receiver) = oneshot::channel();
        self.network_relay
            .send(DaNetworkMsg::Subscribe {
                kind: DaNetworkEventKind::Sampling,
                sender: stream_sender,
            })
            .await
            .map_err(|(error, _)| error)?;
        stream_receiver
            .await
            .map(|stream| {
                tokio_stream::StreamExt::filter_map(stream, |event| match event {
                    DaNetworkEvent::Sampling(event) => {
                        Some(event)
                    }
                    DaNetworkEvent::Verifying(_) => {
                        unreachable!("Subscribirng to sampling events should return a sampling only event stream");
                    }
                }).boxed()
            })
            .map_err(|error| Box::new(error) as DynError)
    }
}

use crate::adapters::network::DispersalNetworkAdapter;
use futures::stream::BoxStream;
use futures::{Stream, StreamExt};
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::{BlobId, DaDispersal};
use nomos_da_network_core::protocols::dispersal::executor::behaviour::DispersalExecutorEvent;
use nomos_da_network_core::{PeerId, SubnetworkId};
use nomos_da_network_service::backends::libp2p::executor::{
    DaNetworkEvent, DaNetworkEventKind, DaNetworkExecutorBackend, ExecutorDaNetworkMessage,
};
use nomos_da_network_service::{DaNetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;
use std::fmt::Debug;
use std::pin::Pin;
use subnetworks_assignations::MembershipHandler;
use tokio::sync::oneshot;

pub struct Libp2pNetworkAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    outbound_relay: OutboundRelay<DaNetworkMsg<DaNetworkExecutorBackend<Membership>>>,
}

#[async_trait::async_trait]
impl<Membership> DispersalNetworkAdapter for Libp2pNetworkAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type NetworkService = NetworkService<DaNetworkExecutorBackend<Membership>>;

    type SubnetworkId = Membership::NetworkId;

    fn new(outbound_relay: OutboundRelay<<Self::NetworkService as ServiceData>::Message>) -> Self {
        Self { outbound_relay }
    }

    async fn disperse(
        &self,
        subnetwork_id: Self::SubnetworkId,
        da_blob: DaBlob,
    ) -> Result<(), DynError> {
        self.outbound_relay
            .send(DaNetworkMsg::Process(
                ExecutorDaNetworkMessage::RequestDispersal {
                    subnetwork_id,
                    da_blob: Box::new(da_blob),
                },
            ))
            .await
            .map_err(|(e, _)| Box::new(e) as DynError)
    }

    async fn dispersal_events_stream(
        &self,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<(BlobId, Self::SubnetworkId), DynError>> + Send>>,
        DynError,
    > {
        let (sender, receiver) = oneshot::channel();
        self.outbound_relay
            .send(DaNetworkMsg::Subscribe {
                kind: DaNetworkEventKind::Dispersal,
                sender,
            })
            .await
            .map_err(|(e, _)| Box::new(e) as DynError)?;
        receiver
            .await
            .map_err(|e| Box::new(e) as DynError)
            .map(|stream| {
                Box::pin(stream.filter_map(|event| async {
                    match event {
                        DaNetworkEvent::Sampling(_) => None,
                        DaNetworkEvent::Verifying(_) => None,
                        DaNetworkEvent::Dispersal(DispersalExecutorEvent::DispersalError {
                            error,
                        }) => Some(Err(Box::new(error) as DynError)),
                        DaNetworkEvent::Dispersal(DispersalExecutorEvent::DispersalSuccess {
                            blob_id,
                            subnetwork_id,
                        }) => Some(Ok((blob_id, subnetwork_id))),
                    }
                }))
                    as BoxStream<'static, Result<(BlobId, Self::SubnetworkId), DynError>>
            })
    }
}

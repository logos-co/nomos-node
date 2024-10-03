pub mod libp2p;
use futures::Stream;
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::{BlobId, DaDispersal};
use nomos_da_network_core::PeerId;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;
use std::pin::Pin;

#[async_trait::async_trait]
pub trait DispersalNetworkAdapter {
    type NetworkService: ServiceData;
    type SubnetworkId;
    fn new(outbound_relay: OutboundRelay<<Self::NetworkService as ServiceData>::Message>) -> Self;

    async fn disperse(
        &self,
        subnetwork_id: Self::SubnetworkId,
        da_blob: DaBlob,
    ) -> Result<(), DynError>;

    async fn dispersal_events_stream(
        &self,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<(BlobId, Self::SubnetworkId), DynError>> + Send>>,
        DynError,
    >;
}

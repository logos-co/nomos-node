pub mod libp2p;
use std::{pin::Pin, time::Duration};

use futures::Stream;
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;
use overwatch::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

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

    async fn get_blob_samples(
        &self,
        blob_id: BlobId,
        subnets: &[SubnetworkId],
        cooldown: Duration,
    ) -> Result<(), DynError>;
}

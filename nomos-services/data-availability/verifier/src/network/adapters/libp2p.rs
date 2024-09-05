// std
use std::fmt::Debug;
use std::marker::PhantomData;
// crates
use futures::Stream;
use kzgrs_backend::common::blob::DaBlob;
use libp2p::PeerId;
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::backends::libp2p::validator::{
    DaNetworkEvent, DaNetworkEventKind, DaNetworkValidatorBackend,
};
use nomos_da_network_service::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use subnetworks_assignations::MembershipHandler;
use tokio_stream::StreamExt;
// internal
use crate::network::NetworkAdapter;

pub struct Libp2pAdapter<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    network_relay:
        OutboundRelay<<NetworkService<DaNetworkValidatorBackend<M>> as ServiceData>::Message>,
    _membership: PhantomData<M>,
}

#[async_trait::async_trait]
impl<M> NetworkAdapter for Libp2pAdapter<M>
where
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type Backend = DaNetworkValidatorBackend<M>;
    type Settings = ();
    type Blob = DaBlob;

    async fn new(
        _settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            network_relay,
            _membership: Default::default(),
        }
    }

    async fn blob_stream(&self) -> Box<dyn Stream<Item = Self::Blob> + Unpin + Send> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.network_relay
            .send(nomos_da_network_service::DaNetworkMsg::Subscribe {
                kind: DaNetworkEventKind::Verifying,
                sender,
            })
            .await
            .expect("Network backend should be ready");

        let receiver = receiver.await.expect("Blob stream should be received");

        let stream = receiver.filter_map(move |msg| match msg {
            DaNetworkEvent::Verifying(blob) => Some(*blob),
            _ => None,
        });

        Box::new(Box::pin(stream))
    }
}

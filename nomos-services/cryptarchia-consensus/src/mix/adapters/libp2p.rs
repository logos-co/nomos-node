use std::{hash::Hash, marker::PhantomData};

use nomos_core::{block::Block, wire};
use nomos_mix_service::{
    backends::libp2p::Libp2pMixBackend, network::NetworkAdapter, MixService, ServiceMessage,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{messages::NetworkMessage, mix::MixAdapter};

#[derive(Clone)]
pub struct LibP2pAdapter<Network, Tx, BlobCert>
where
    Network: NetworkAdapter,
    Network::BroadcastSettings: Clone,
    Tx: Clone + Eq + Hash,
    BlobCert: Clone + Eq + Hash,
{
    settings: LibP2pAdapterSettings<Network::BroadcastSettings>,
    mix_relay: OutboundRelay<<MixService<Libp2pMixBackend, Network> as ServiceData>::Message>,
    _tx: PhantomData<Tx>,
    _blob_cert: PhantomData<BlobCert>,
}

#[async_trait::async_trait]
impl<Network, Tx, BlobCert> MixAdapter for LibP2pAdapter<Network, Tx, BlobCert>
where
    Network: NetworkAdapter + 'static,
    Network::BroadcastSettings: Clone,
    Tx: Serialize + DeserializeOwned + Clone + Eq + Hash + Send + Sync + 'static,
    BlobCert: Serialize + DeserializeOwned + Clone + Eq + Hash + Send + Sync + 'static,
{
    type Settings = LibP2pAdapterSettings<Network::BroadcastSettings>;
    type Backend = Libp2pMixBackend;
    type Network = Network;
    type Tx = Tx;
    type BlobCertificate = BlobCert;

    async fn new(
        settings: Self::Settings,
        mix_relay: OutboundRelay<
            <MixService<Self::Backend, Self::Network> as ServiceData>::Message,
        >,
    ) -> Self {
        // this wait seems to be helpful in some cases since we give the time
        // to the network to establish connections before we start sending messages
        // TODO: Remove this once we have the status system to await for service readiness
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Self {
            settings,
            mix_relay,
            _tx: PhantomData,
            _blob_cert: PhantomData,
        }
    }

    async fn mix(&self, block: Block<Self::Tx, Self::BlobCertificate>) {
        if let Err((e, msg)) = self
            .mix_relay
            .send(ServiceMessage::Mix(nomos_mix_service::NetworkMessage {
                message: wire::serialize(&NetworkMessage::Block(block)).unwrap(),
                broadcast_settings: self.settings.broadcast_settings.clone(),
            }))
            .await
        {
            tracing::error!("error sending message to mix network: {e}: {msg:?}",);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibP2pAdapterSettings<BroadcastSettings> {
    pub broadcast_settings: BroadcastSettings,
}

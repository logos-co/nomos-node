use nomos_network::{
    backends::libp2p::{Command, Libp2p},
    NetworkMsg, NetworkService,
};
use overwatch::services::{relay::OutboundRelay, ServiceData};
use serde::{Deserialize, Serialize};

use super::NetworkAdapter;

/// A network adapter for the network service that uses libp2p backend.
#[derive(Clone)]
pub struct Libp2pAdapter<RuntimeServiceId> {
    network_relay:
        OutboundRelay<<NetworkService<Libp2p, RuntimeServiceId> as ServiceData>::Message>,
}

/// Settings used to broadcast messages to the network service that uses libp2p
/// backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Libp2pBroadcastSettings {
    pub topic: String,
}

#[async_trait::async_trait]
impl<RuntimeServiceId> NetworkAdapter<RuntimeServiceId> for Libp2pAdapter<RuntimeServiceId> {
    type Backend = Libp2p;
    type BroadcastSettings = Libp2pBroadcastSettings;

    fn new(
        network_relay: OutboundRelay<
            <NetworkService<Self::Backend, RuntimeServiceId> as ServiceData>::Message,
        >,
    ) -> Self {
        Self { network_relay }
    }

    async fn broadcast(&self, message: Vec<u8>, broadcast_settings: Self::BroadcastSettings) {
        if let Err((e, _)) = self
            .network_relay
            .send(NetworkMsg::Process(Command::Broadcast {
                topic: broadcast_settings.topic.clone(),
                message: message.into_boxed_slice(),
            }))
            .await
        {
            tracing::error!("error broadcasting message: {e}");
        }
    }
}

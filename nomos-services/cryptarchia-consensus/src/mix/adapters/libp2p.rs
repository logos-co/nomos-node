use std::pin::Pin;

use futures::{Stream, StreamExt};
use nomos_mix_service::{
    backends::libp2p::{
        Libp2pNetworkBackend, Libp2pNetworkBackendEvent, Libp2pNetworkBackendEventKind,
        Libp2pNetworkBackendMessage,
    },
    NetworkMsg, NetworkService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

use crate::mix::NetworkAdapter;

#[derive(Clone)]
pub struct LibP2pAdapter {
    network_relay: OutboundRelay<<NetworkService<Libp2pNetworkBackend> as ServiceData>::Message>,
}

#[async_trait::async_trait]
impl NetworkAdapter for LibP2pAdapter {
    type Backend = Libp2pNetworkBackend;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        // this wait seems to be helpful in some cases since we give the time
        // to the network to establish connections before we start sending messages
        // TODO: Remove this once we have the status system to await for service readiness
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Self { network_relay }
    }

    async fn mix(&self, message: Vec<u8>) {
        if let Err((e, msg)) = self
            .network_relay
            .send(NetworkMsg::Process(Libp2pNetworkBackendMessage::Mix(
                message,
            )))
            .await
        {
            tracing::error!("error sending message to mix network: {e}: {msg:?}",);
        }
    }

    async fn mixed_messages_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, DynError> {
        let (stream_sender, stream_receiver) = tokio::sync::oneshot::channel();
        self.network_relay
            .send(NetworkMsg::Subscribe {
                kind: Libp2pNetworkBackendEventKind::FullyMixedMessage,
                sender: stream_sender,
            })
            .await
            .map_err(|(error, _)| error)?;
        stream_receiver
            .await
            .map(|stream| {
                tokio_stream::StreamExt::filter_map(stream, |event| match event {
                    Libp2pNetworkBackendEvent::FullyMixedMessage(msg) => Some(msg),
                })
                .boxed()
            })
            .map_err(|error| Box::new(error) as DynError)
    }
}

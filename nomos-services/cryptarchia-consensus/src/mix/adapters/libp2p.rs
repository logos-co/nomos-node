use nomos_mix_service::{
    backends::libp2p::{
        Libp2pNetworkBackend, Libp2pNetworkBackendEvent, Libp2pNetworkBackendEventKind,
        Libp2pNetworkBackendMessage,
    },
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use tokio::sync::broadcast::{self, error::RecvError};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::mix::{BoxedStream, NetworkAdapter};

const BUFFER_SIZE: usize = 64;

#[derive(Clone)]
pub struct LibP2pAdapter {
    network_relay: OutboundRelay<<NetworkService<Libp2pNetworkBackend> as ServiceData>::Message>,
    mixed_msgs: broadcast::Sender<Vec<u8>>,
}

#[async_trait::async_trait]
impl NetworkAdapter for LibP2pAdapter {
    type Backend = Libp2pNetworkBackend;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        let relay = network_relay.clone();
        let mixed_msgs = broadcast::Sender::new(BUFFER_SIZE);
        let mixed_msgs_sender = mixed_msgs.clone();
        // this wait seems to be helpful in some cases since we give the time
        // to the network to establish connections before we start sending messages
        // TODO: Remove this once we have the status system to await for service readiness
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        tokio::spawn(async move {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            if let Err((e, _)) = relay
                .send(NetworkMsg::Subscribe {
                    kind: Libp2pNetworkBackendEventKind::FullyMixedMessage,
                    sender,
                })
                .await
            {
                tracing::error!("error subscribing to incoming mixed msgs: {e}");
            }

            let mut incoming_mixed_msgs = receiver.await.unwrap();
            loop {
                match incoming_mixed_msgs.recv().await {
                    Ok(Libp2pNetworkBackendEvent::FullyMixedMessage(msg)) => {
                        tracing::debug!("received a fully mixed message");
                        if let Err(e) = mixed_msgs_sender.send(msg) {
                            tracing::error!("error sending mixed message to consensus: {e}");
                        }
                    }
                    Err(RecvError::Lagged(n)) => {
                        tracing::error!("lagged messages: {n}")
                    }
                    Err(RecvError::Closed) => unreachable!(),
                }
            }
        });

        Self {
            network_relay,
            mixed_msgs,
        }
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

    async fn mixed_messages_stream(&self) -> BoxedStream<Vec<u8>> {
        Box::new(BroadcastStream::new(self.mixed_msgs.subscribe()).filter_map(Result::ok))
    }
}

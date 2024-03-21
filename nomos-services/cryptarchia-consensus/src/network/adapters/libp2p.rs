// std
use std::hash::Hash;
// crates
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
// internal
use crate::network::{messages::NetworkMessage, BoxedStream, NetworkAdapter};
use nomos_core::{block::Block, wire};
use nomos_network::{
    backends::libp2p::{Command, Event, EventKind, Libp2p},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};

const TOPIC: &str = "/cryptarchia/proto";
const BUFFER_SIZE: usize = 64;
type Relay<T> = OutboundRelay<<NetworkService<T> as ServiceData>::Message>;

#[derive(Clone)]
pub struct LibP2pAdapter<Tx, BlobCert>
where
    Tx: Clone + Eq + Hash,
    BlobCert: Clone + Eq + Hash,
{
    network_relay: OutboundRelay<<NetworkService<Libp2p> as ServiceData>::Message>,
    blocks: tokio::sync::broadcast::Sender<Block<Tx, BlobCert>>,
}

impl<Tx, BlobCert> LibP2pAdapter<Tx, BlobCert>
where
    Tx: Clone + Eq + Hash + Serialize,
    BlobCert: Clone + Eq + Hash + Serialize,
{
    async fn subscribe(relay: &Relay<Libp2p>, topic: &str) {
        if let Err((e, _)) = relay
            .send(NetworkMsg::Process(Command::Subscribe(topic.into())))
            .await
        {
            tracing::error!("error subscribing to {topic}: {e}");
        };
    }
}

#[async_trait::async_trait]
impl<Tx, BlobCert> NetworkAdapter for LibP2pAdapter<Tx, BlobCert>
where
    Tx: Serialize + DeserializeOwned + Clone + Eq + Hash + Send + 'static,
    BlobCert: Serialize + DeserializeOwned + Clone + Eq + Hash + Send + 'static,
{
    type Backend = Libp2p;
    type Tx = Tx;
    type BlobCertificate = BlobCert;

    async fn new(network_relay: Relay<Libp2p>) -> Self {
        let relay = network_relay.clone();
        Self::subscribe(&relay, TOPIC).await;
        let blocks = tokio::sync::broadcast::Sender::new(BUFFER_SIZE);
        let blocks_sender = blocks.clone();
        tracing::debug!("Starting up...");
        // this wait seems to be helpful in some cases since we give the time
        // to the network to establish connections before we start sending messages
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        // TODO: maybe we need the runtime handle here?
        tokio::spawn(async move {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            if let Err((e, _)) = relay
                .send(NetworkMsg::Subscribe {
                    kind: EventKind::Message,
                    sender,
                })
                .await
            {
                tracing::error!("error subscribing to incoming messages: {e}");
            }

            let mut incoming_messages = receiver.await.unwrap();
            loop {
                match incoming_messages.recv().await {
                    Ok(Event::Message(message)) => {
                        match nomos_core::wire::deserialize(&message.data) {
                            Ok(msg) => match msg {
                                NetworkMessage::Block(block) => {
                                    tracing::debug!("received block {:?}", block.header().id());
                                    if let Err(err) = blocks_sender.send(block) {
                                        tracing::error!("error sending block to consensus: {err}");
                                    }
                                }
                            },
                            _ => tracing::debug!("unrecognized gossipsub message"),
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
            blocks,
        }
    }

    async fn blocks_stream(&self) -> BoxedStream<Block<Self::Tx, Self::BlobCertificate>> {
        Box::new(BroadcastStream::new(self.blocks.subscribe()).filter_map(Result::ok))
    }

    async fn broadcast(&self, message: NetworkMessage<Self::Tx, Self::BlobCertificate>) {
        if let Err((e, message)) = self
            .network_relay
            .send(NetworkMsg::Process(Command::Broadcast {
                message: wire::serialize(&message).unwrap().into_boxed_slice(),
                topic: TOPIC.into(),
            }))
            .await
        {
            tracing::error!("error broadcasting {message:?}: {e}");
        };
    }
}

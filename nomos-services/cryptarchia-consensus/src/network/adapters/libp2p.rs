// std
use overwatch_rs::DynError;
use std::hash::Hash;
use std::marker::PhantomData;
// crates
use serde::{de::DeserializeOwned, Serialize};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
// internal
use crate::network::{messages::NetworkMessage, BoxedStream, NetworkAdapter};
use nomos_core::{block::Block, wire};
use nomos_network::{
    backends::libp2p::{Command, Event, EventKind, Libp2p},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

const TOPIC: &str = "/cryptarchia/proto";
type Relay<T> = OutboundRelay<<NetworkService<T> as ServiceData>::Message>;

#[derive(Clone)]
pub struct LibP2pAdapter<Tx, BlobCert>
where
    Tx: Clone + Eq + Hash,
    BlobCert: Clone + Eq + Hash,
{
    network_relay: OutboundRelay<<NetworkService<Libp2p> as ServiceData>::Message>,
    _phantom_tx: PhantomData<Tx>,
    _blob_cert: PhantomData<BlobCert>,
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
    Tx: Serialize + DeserializeOwned + Clone + Eq + Hash + Send + Sync + 'static,
    BlobCert: Serialize + DeserializeOwned + Clone + Eq + Hash + Send + Sync + 'static,
{
    type Backend = Libp2p;
    type Tx = Tx;
    type BlobCertificate = BlobCert;

    async fn new(network_relay: Relay<Libp2p>) -> Self {
        let relay = network_relay.clone();
        Self::subscribe(&relay, TOPIC).await;
        tracing::debug!("Starting up...");
        // this wait seems to be helpful in some cases since we give the time
        // to the network to establish connections before we start sending messages
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        Self {
            network_relay,
            _phantom_tx: Default::default(),
            _blob_cert: Default::default(),
        }
    }

    async fn blocks_stream(
        &self,
    ) -> Result<BoxedStream<Block<Self::Tx, Self::BlobCertificate>>, DynError> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((e, _)) = self
            .network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
        {
            return Err(Box::new(e));
        }
        Ok(Box::new(
            BroadcastStream::new(receiver.await.map_err(Box::new)?).filter_map(|message| {
                match message {
                    Ok(Event::Message(message)) => match wire::deserialize(&message.data) {
                        Ok(msg) => match msg {
                            NetworkMessage::Block(block) => {
                                tracing::debug!("received block {:?}", block.header().id());
                                Some(block)
                            }
                        },
                        _ => {
                            tracing::debug!("unrecognized gossipsub message");
                            None
                        }
                    },
                    Err(BroadcastStreamRecvError::Lagged(n)) => {
                        tracing::error!("lagged messages: {n}");
                        None
                    }
                }
            }),
        ))
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

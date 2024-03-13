// crates
use futures::Stream;
use serde::{de::DeserializeOwned, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
// internal
use crate::network::NetworkAdapter;
use nomos_core::wire;
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2p as Backend;
use nomos_network::backends::libp2p::{Command, Event, EventKind, Message, TopicHash};
#[cfg(feature = "mixnet")]
use nomos_network::backends::mixnet::MixnetNetworkBackend as Backend;
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub struct P2pAdapter<Item, Key> {
    network_relay: OutboundRelay<<NetworkService<Backend> as ServiceData>::Message>,
    settings: Settings<Key, Item>,
}

#[async_trait::async_trait]
impl<Item, Key> NetworkAdapter for P2pAdapter<Item, Key>
where
    Item: DeserializeOwned + Serialize + Send + Sync + 'static + Clone,
    Key: Clone + Send + Sync + 'static,
{
    type Backend = Backend;
    type Settings = Settings<Key, Item>;
    type Item = Item;
    type Key = Key;

    async fn new(
        settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        network_relay
            .send(NetworkMsg::Process(Command::Subscribe(
                settings.topic.clone(),
            )))
            .await
            .expect("Network backend should be ready");
        Self {
            network_relay,
            settings,
        }
    }
    async fn transactions_stream(
        &self,
    ) -> Box<dyn Stream<Item = (Self::Key, Self::Item)> + Unpin + Send> {
        let topic_hash = TopicHash::from_raw(self.settings.topic.clone());
        let id = self.settings.id;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        self.network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
            .expect("Network backend should be ready");
        let receiver = receiver.await.unwrap();
        Box::new(Box::pin(BroadcastStream::new(receiver).filter_map(
            move |message| match message {
                Ok(Event::Message(Message { data, topic, .. })) if topic == topic_hash => {
                    match wire::deserialize::<Item>(&data) {
                        Ok(item) => Some((id(&item), item)),
                        Err(e) => {
                            tracing::debug!("Unrecognized message: {e}");
                            None
                        }
                    }
                }
                _ => None,
            },
        )))
    }

    async fn send(&self, item: Item) {
        if let Ok(wire) = wire::serialize(&item) {
            if let Err((e, _)) = self
                .network_relay
                .send(NetworkMsg::Process(Command::Broadcast {
                    topic: self.settings.topic.clone(),
                    message: wire.into(),
                }))
                .await
            {
                tracing::error!("failed to send item to topic: {e}");
            }
        } else {
            tracing::error!("Failed to serialize item");
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings<K, V> {
    pub topic: String,
    pub id: fn(&V) -> K,
}

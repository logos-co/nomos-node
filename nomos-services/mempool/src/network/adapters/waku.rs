// std
use std::marker::PhantomData;
// crates
use futures::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::TransactionMsg;
use crate::network::NetworkAdapter;
use nomos_core::wire;
use nomos_network::backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::Serialize;
use waku_bindings::{Encoding, WakuContentTopic, WakuPubSubTopic};

pub const WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
    WakuPubSubTopic::new("CarnotSim", Encoding::Proto);

pub const WAKU_CARNOT_TX_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new("CarnotSim", 1, "CarnotTx", Encoding::Proto);

pub struct WakuAdapter<Item, Key> {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
    _item: PhantomData<Item>,
    _key: PhantomData<Key>,
}

#[async_trait::async_trait]
impl<Item, Key> NetworkAdapter for WakuAdapter<Item, Key>
where
    Item: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    type Backend = Waku;
    type Settings = ();
    type Item = Item;
    // TODO: implement real key
    type Key = ();

    async fn new(
        _settings: Self::Settings,
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        // Subscribe to the carnot pubsub topic
        if let Err((e, _)) = network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::RelaySubscribe {
                topic: WAKU_CARNOT_PUB_SUB_TOPIC.clone(),
            }))
            .await
        {
            // We panic, but as we could try to reconnect later it should not be
            // a problem. But definitely something to consider.
            panic!("Couldn't send subscribe message to the network service: {e}");
        };
        Self {
            network_relay,
            _tx: Default::default(),
        }
    }

    async fn transactions_stream(&self) -> Box<dyn Stream<Item = Self::Item> + Unpin + Send> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
        {
            todo!("log error");
        };
        let receiver = receiver.await.unwrap();
        Box::new(Box::pin(BroadcastStream::new(receiver).filter_map(
            |event| async move {
                match event {
                    Ok(NetworkEvent::RawMessage(message)) => {
                        if message.content_topic() == &WAKU_CARNOT_TX_CONTENT_TOPIC {
                            let item: Self::Item =
                                wire::deserializer(message.payload()).deserialize().unwrap();
                            // TODO: implement real key
                            Some(((), item))
                        } else {
                            None
                        }
                    }
                    Err(_e) => None,
                }
            },
        )))
    }
}

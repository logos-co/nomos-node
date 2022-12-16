use bincode::config::{Fixint, LittleEndian, NoLimit, WriteFixedArrayLength};
use std::marker::PhantomData;
// std
// crates
use futures::{Stream, StreamExt};
use once_cell::sync::Lazy;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::TransactionMsg;
use crate::network::NetworkAdapter;
use nomos_network::backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use waku_bindings::{Encoding, WakuContentTopic, WakuPubSubTopic};

static WAKU_CARNOT_PUB_SUB_TOPIC: Lazy<WakuPubSubTopic> =
    Lazy::new(|| WakuPubSubTopic::new("CarnotSim".to_string(), Encoding::Proto));

static WAKU_CARNOT_TX_CONTENT_TOPIC: Lazy<WakuContentTopic> = Lazy::new(|| WakuContentTopic {
    application_name: "CarnotSim".to_string(),
    version: 1,
    content_topic_name: "CarnotTx".to_string(),
    encoding: Encoding::Proto,
});

pub struct WakuAdapter<Tx, Id> {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
    _tx: PhantomData<(Tx, Id)>,
}

#[async_trait::async_trait]
impl<Tx, Id> NetworkAdapter for WakuAdapter<Tx, Id>
where
    Tx: Serialize + DeserializeOwned + Send + Sync + 'static,
    Id: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    type Backend = Waku;
    type Tx = Tx;
    type Id = Id;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        // Subscribe to the carnot pubsub topic
        if let Err((_, _e)) = network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::RelaySubscribe {
                topic: WAKU_CARNOT_PUB_SUB_TOPIC.clone(),
            }))
            .await
        {
            // We could actually panic, but as we could try to reconnect later it should not be
            // a problem. But definitely something to consider.
            todo!("log error");
        };
        Self {
            network_relay,
            _tx: Default::default(),
        }
    }
    async fn transactions_stream(
        &self,
    ) -> Box<dyn Stream<Item = (Self::Tx, Self::Id)> + Unpin + Send> {
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
                        if message.content_topic().content_topic_name
                            == WAKU_CARNOT_TX_CONTENT_TOPIC.content_topic_name
                        {
                            let (tx, _): (TransactionMsg<Self::Tx, Self::Id>, _) =
                                // TODO: This should be temporary, we can probably extract this so we can use/try/test a variety of encodings
                                bincode::serde::decode_from_slice(
                                    message.payload(),
                                    bincode::config::Configuration::<
                                        LittleEndian,
                                        Fixint,
                                        WriteFixedArrayLength,
                                        NoLimit,
                                    >::default(),
                                )
                                .unwrap();
                            Some((tx.tx, tx.id))
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

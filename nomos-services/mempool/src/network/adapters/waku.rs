use bincode::config::{Fixint, LittleEndian, NoLimit, WriteFixedArrayLength};
use std::marker::PhantomData;
// std
// crates
use futures::{Stream, StreamExt};
use serde::de::DeserializeOwned;
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::messages::TransactionMsg;
use crate::network::NetworkAdapter;
use nomos_network::backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::Serialize;
use waku_bindings::{Encoding, WakuContentTopic, WakuMessage, WakuPubSubTopic};

const WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
    WakuPubSubTopic::new("CarnotSim", Encoding::Proto);

const WAKU_CARNOT_TX_CONTENT_TOPIC: WakuContentTopic =
    WakuContentTopic::new("CarnotSim", 1, "CarnotTx", Encoding::Proto);

pub struct WakuAdapter<Tx> {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
    _tx: PhantomData<Tx>,
}

#[async_trait::async_trait]
impl<Tx> NetworkAdapter for WakuAdapter<Tx>
where
    Tx: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    type Backend = Waku;
    type Tx = Tx;

    async fn new(
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
    async fn transactions_stream(&self) -> Box<dyn Stream<Item = Self::Tx> + Unpin + Send> {
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
                            let (tx, _): (TransactionMsg<Self::Tx>, _) =
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
                            Some(tx.tx)
                        } else {
                            None
                        }
                    }
                    Err(_e) => None,
                }
            },
        )))
    }

    async fn send_transaction(&self, tx: Self::Tx) {
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message: WakuMessage::new(
                    bincode::serde::encode_to_vec(
                        &TransactionMsg { tx },
                        bincode::config::standard(),
                    )
                    .unwrap(),
                    WAKU_CARNOT_TX_CONTENT_TOPIC.clone(),
                    1,
                    chrono::Utc::now().timestamp() as usize,
                ),
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
            }))
            .await
        {
            todo!("log error");
        };
    }
}

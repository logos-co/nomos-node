// std
use std::marker::PhantomData;
// crates
use futures::Stream;
use serde::{de::DeserializeOwned, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::log::error;

// internal
use crate::network::messages::TransactionMsg;
use crate::network::NetworkAdapter;
use nomos_core::wire;
use nomos_network::backends::libp2p::{Command, Event, EventKind, Libp2p, Message, TopicHash};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub const CARNOT_TX_TOPIC: &str = "CarnotTx";

pub struct Libp2pAdapter<Tx> {
    network_relay: OutboundRelay<<NetworkService<Libp2p> as ServiceData>::Message>,
    _tx: PhantomData<Tx>,
}

#[async_trait::async_trait]
impl<Tx> NetworkAdapter for Libp2pAdapter<Tx>
where
    Tx: DeserializeOwned + Serialize + Send + Sync + 'static,
{
    type Backend = Libp2p;
    type Tx = Tx;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        network_relay
            .send(NetworkMsg::Process(Command::Subscribe(
                CARNOT_TX_TOPIC.to_string(),
            )))
            .await
            .expect("Network backend should be ready");
        Self {
            network_relay,
            _tx: PhantomData,
        }
    }
    async fn transactions_stream(&self) -> Box<dyn Stream<Item = Self::Tx> + Unpin + Send> {
        let topic_hash = TopicHash::from_raw(CARNOT_TX_TOPIC);
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
                    match wire::deserialize::<TransactionMsg<Tx>>(&data) {
                        Ok(msg) => Some(msg.tx),
                        Err(e) => {
                            error!("Unrecognized Tx message: {e}");
                            None
                        }
                    }
                }
                _ => None,
            },
        )))
    }
}

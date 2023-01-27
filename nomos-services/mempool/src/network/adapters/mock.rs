// std
use std::marker::PhantomData;

// crates
use futures::{Stream, StreamExt};
use nomos_network::backends::mock::{EventKind, Mock, MockBackendMessage, NetworkEvent};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use serde::de::DeserializeOwned;
use tokio_stream::wrappers::BroadcastStream;

// internal
use crate::network::NetworkAdapter;

const MOCK_PUB_SUB_TOPIC: &str = "MockPubSubTopic";
const MOCK_CONTENT_TOPIC: &str = "MockContentTopic";

pub struct MockAdapter<Tx> {
    network_relay: OutboundRelay<<NetworkService<Mock> as ServiceData>::Message>,
    _tx: PhantomData<Tx>,
}

#[async_trait::async_trait]
impl<Tx> NetworkAdapter for MockAdapter<Tx>
where
    Tx: From<String> + DeserializeOwned + Send + Sync + 'static,
{
    type Backend = Mock;
    type Tx = Tx;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        // Subscribe to the carnot topic 0
        if let Err((e, _)) = network_relay
            .send(NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                topic: MOCK_PUB_SUB_TOPIC,
            }))
            .await
        {
            // We panic, but as we could try to reconnect later it should not be
            // a problem. But definitely something to consider.
            panic!(
                "Couldn't send subscribe message to the network service: {}",
                e
            );
        };
        Self {
            network_relay,
            _tx: Default::default(),
        }
    }
    async fn transactions_stream(&self) -> Box<dyn Stream<Item = Self::Tx> + Unpin + Send> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, e)) = self
            .network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
        {
            tracing::error!(err = ?e);
        };
        let receiver = receiver.await.unwrap();
        Box::new(Box::pin(BroadcastStream::new(receiver).filter_map(
            |event| async move {
                match event {
                    Ok(NetworkEvent::RawMessage(message)) => {
                        if message.content_topic().content_topic_name == MOCK_CONTENT_TOPIC {
                            Some(Tx::from(message.payload()))
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

#[cfg(test)]
mod tests {}

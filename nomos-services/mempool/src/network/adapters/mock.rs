// std

// crates
use futures::{Stream, StreamExt};
use nomos_core::tx::mock::MockTransactionMsg;
use nomos_network::backends::mock::{
    EventKind, Mock, MockBackendMessage, MockContentTopic, NetworkEvent,
};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use tokio_stream::wrappers::BroadcastStream;

// internal
use crate::network::NetworkAdapter;

pub const MOCK_PUB_SUB_TOPIC: &str = "MockPubSubTopic";
pub const MOCK_CONTENT_TOPIC: &str = "MockContentTopic";
pub const MOCK_TX_CONTENT_TOPIC: MockContentTopic = MockContentTopic::new("Mock", 1, "Tx");

pub struct MockAdapter {
    network_relay: OutboundRelay<<NetworkService<Mock> as ServiceData>::Message>,
}

#[async_trait::async_trait]
impl NetworkAdapter for MockAdapter {
    type Backend = Mock;
    type Tx = MockTransactionMsg;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        // send message to boot the network producer
        if let Err(e) = network_relay
            .send(NetworkMsg::Process(MockBackendMessage::BootProducer {
                spawner: Box::new(move |fut| {
                    tokio::spawn(fut);
                    Ok(())
                }),
            }))
            .await
        {
            panic!(
                "Couldn't send boot producer message to the network service: {:?}",
                e.0
            );
        }

        if let Err((e, _)) = network_relay
            .send(NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                topic: MOCK_PUB_SUB_TOPIC,
            }))
            .await
        {
            panic!("Couldn't send subscribe message to the network service: {e}",);
        };
        Self { network_relay }
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
        let outbound = self.network_relay.clone();
        Box::new(Box::pin(BroadcastStream::new(receiver).filter_map(
            move |event| {
                let outbound = outbound.clone();
                async move {
                    match event {
                        Ok(NetworkEvent::RawMessage(message)) => {
                            tracing::info!("Received message: {:?}", message.payload());
                            if message.content_topic() == MOCK_TX_CONTENT_TOPIC {
                                Some(MockTransactionMsg { msg: message })
                            } else {
                                // sent assert message to check if we got the expected message
                                let (tx, rx) = tokio::sync::oneshot::channel();

                                if let Err(e) = outbound
                                    .send(NetworkMsg::Process(MockBackendMessage::Assert {
                                        msg: message,
                                        tx,
                                    }))
                                    .await
                                {
                                    tracing::error!(err = ?e, "fail to send assert message");
                                }

                                if let Err(e) = rx.await {
                                    tracing::error!(err = ?e, "fail to receive assert message response");
                                }
                                None
                            }
                        }
                        Err(_e) => None,
                    }
                }
            },
        )))
    }
}

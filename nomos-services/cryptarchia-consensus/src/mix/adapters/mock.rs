use std::pin::Pin;

use futures::{Stream, StreamExt};
use nomos_mix_service::{
    backends::mock::{Mock, MockEvent, MockEventKind, MockMessage},
    NetworkMsg, NetworkService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

use crate::mix::NetworkAdapter;

#[derive(Clone)]
pub struct MockAdapter {
    network_relay: OutboundRelay<<NetworkService<Mock> as ServiceData>::Message>,
}

#[async_trait::async_trait]
impl NetworkAdapter for MockAdapter {
    type Backend = Mock;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self { network_relay }
    }

    async fn mix(&self, message: Vec<u8>) {
        if let Err((e, msg)) = self
            .network_relay
            .send(NetworkMsg::Process(MockMessage::Mix(message)))
            .await
        {
            tracing::error!("error sending message to mix network: {e}: {msg:?}",);
        }
    }

    async fn mixed_messages_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Vec<u8>> + Send>>, DynError> {
        let (stream_sender, stream_receiver) = tokio::sync::oneshot::channel();
        self.network_relay
            .send(NetworkMsg::Subscribe {
                kind: MockEventKind::FullyMixedMessage,
                sender: stream_sender,
            })
            .await
            .map_err(|(error, _)| error)?;
        stream_receiver
            .await
            .map(|stream| {
                tokio_stream::StreamExt::filter_map(stream, |event| match event {
                    MockEvent::FullyMixedMessage(msg) => Some(msg),
                })
                .boxed()
            })
            .map_err(|error| Box::new(error) as DynError)
    }
}

// std
use futures::Stream;
use overwatch_rs::DynError;
use std::marker::PhantomData;
// crates

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
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tracing::debug;

pub const NOMOS_DA_TOPIC: &str = "NomosDa";

pub struct P2pAdapter<B, A> {
    network_relay: OutboundRelay<<NetworkService<Backend> as ServiceData>::Message>,
    _blob: PhantomData<B>,
    _attestation: PhantomData<A>,
}

impl<B, A> P2pAdapter<B, A>
where
    B: Serialize + DeserializeOwned + Send + Sync + 'static,
    A: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    async fn stream_for<E: DeserializeOwned>(&self) -> Box<dyn Stream<Item = E> + Unpin + Send> {
        let topic_hash = TopicHash::from_raw(NOMOS_DA_TOPIC);
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
            move |msg| match msg {
                Ok(Event::Message(Message { topic, data, .. })) if topic == topic_hash => {
                    match wire::deserialize::<E>(&data) {
                        Ok(msg) => Some(msg),
                        Err(e) => {
                            debug!("Unrecognized message: {e}");
                            None
                        }
                    }
                }
                _ => None,
            },
        )))
    }

    async fn send<E: Serialize>(&self, data: E) -> Result<(), DynError> {
        let message = wire::serialize(&data)?.into_boxed_slice();
        self.network_relay
            .send(NetworkMsg::Process(Command::Broadcast {
                topic: NOMOS_DA_TOPIC.to_string(),
                message,
            }))
            .await
            .map_err(|(e, _)| Box::new(e) as DynError)
    }
}

#[async_trait::async_trait]
impl<B, A> NetworkAdapter for P2pAdapter<B, A>
where
    B: Serialize + DeserializeOwned + Send + Sync + 'static,
    A: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    type Backend = Backend;
    type Blob = B;
    type Attestation = A;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        network_relay
            .send(NetworkMsg::Process(Command::Subscribe(
                NOMOS_DA_TOPIC.to_string(),
            )))
            .await
            .expect("Network backend should be ready");
        Self {
            network_relay,
            _blob: Default::default(),
            _attestation: Default::default(),
        }
    }

    async fn blob_stream(&self) -> Box<dyn Stream<Item = Self::Blob> + Unpin + Send> {
        self.stream_for::<Self::Blob>().await
    }

    async fn attestation_stream(&self) -> Box<dyn Stream<Item = Self::Attestation> + Unpin + Send> {
        self.stream_for::<Self::Attestation>().await
    }

    async fn send_attestation(&self, attestation: Self::Attestation) -> Result<(), DynError> {
        self.send(attestation).await
    }

    async fn send_blob(&self, blob: Self::Blob) -> Result<(), DynError> {
        self.send(blob).await
    }
}

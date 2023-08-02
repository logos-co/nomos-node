// std
use std::marker::PhantomData;
// crates
use futures::Stream;
use serde::{de::DeserializeOwned, Serialize};
// internal
use crate::network::NetworkAdapter;
use nomos_network::backends::libp2p::Libp2p;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;

pub struct Libp2pAdapter<Tx> {
    _network_relay: OutboundRelay<<NetworkService<Libp2p> as ServiceData>::Message>,
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
        _network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self {
            _network_relay,
            _tx: PhantomData,
        }
    }
    async fn transactions_stream(&self) -> Box<dyn Stream<Item = Self::Tx> + Unpin + Send> {
        // TODO
        Box::new(futures::stream::empty())
    }
}

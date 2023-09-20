pub mod backend;
pub mod network;

// std
use std::fmt::{Debug, Error, Formatter};

// crates
use futures::StreamExt;
use tokio::sync::oneshot::Sender;
// internal
use crate::network::NetworkAdapter;
use backend::MemPool;
use nomos_core::block::BlockId;
use nomos_network::NetworkService;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

pub struct MempoolService<N, P>
where
    N: NetworkAdapter<Item = P::Item, Key = P::Key>,
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N::Backend>>,
    pool: P,
}

pub struct MempoolMetrics {
    pub pending_items: usize,
    pub last_item_timestamp: u64,
}

pub enum MempoolMsg<Item, Key> {
    Add {
        item: Item,
        key: Key,
        reply_channel: Sender<Result<(), ()>>,
    },
    View {
        ancestor_hint: BlockId,
        reply_channel: Sender<Box<dyn Iterator<Item = Item> + Send>>,
    },
    Prune {
        ids: Vec<Key>,
    },
    #[cfg(test)]
    BlockItems {
        block: BlockId,
        reply_channel: Sender<Option<Box<dyn Iterator<Item = Item> + Send>>>,
    },
    MarkInBlock {
        ids: Vec<Key>,
        block: BlockId,
    },
    Metrics {
        reply_channel: Sender<MempoolMetrics>,
    },
}

impl<Item, Key> Debug for MempoolMsg<Item, Key>
where
    Item: Debug,
    Key: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Self::View { ancestor_hint, .. } => {
                write!(f, "MempoolMsg::View {{ ancestor_hint: {ancestor_hint:?}}}")
            }
            Self::Add { item, .. } => write!(f, "MempoolMsg::Add{{item: {item:?}}}"),
            Self::Prune { ids } => write!(f, "MempoolMsg::Prune{{ids: {ids:?}}}"),
            Self::MarkInBlock { ids, block } => {
                write!(
                    f,
                    "MempoolMsg::MarkInBlock{{ids: {ids:?}, block: {block:?}}}"
                )
            }
            #[cfg(test)]
            Self::BlockItem { block, .. } => {
                write!(f, "MempoolMsg::BlockItem{{block: {block:?}}}")
            }
            Self::Metrics { .. } => write!(f, "MempoolMsg::Metrics"),
        }
    }
}

impl<Item: 'static, Key: 'static> RelayMessage for MempoolMsg<Item, Key> {}

impl<N, P> ServiceData for MempoolService<N, P>
where
    N: NetworkAdapter<Item = P::Item, Key = P::Key>,
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
{
    const SERVICE_ID: ServiceId = "Mempool";
    type Settings = Settings<P::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MempoolMsg<<P as MemPool>::Item, <P as MemPool>::Key>;
}

#[async_trait::async_trait]
impl<N, P> ServiceCore for MempoolService<N, P>
where
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    N: NetworkAdapter<Item = P::Item, Key = P::Key> + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let pool_settings = service_state.settings_reader.get_updated_settings();
        Ok(Self {
            service_state,
            network_relay,
            pool: P::new(pool_settings),
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            network_relay,
            mut pool,
        } = self;

        let network_relay: OutboundRelay<_> = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let adapter = N::new(network_relay).await;
        let mut network_txs = adapter.transactions_stream().await;

        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    match msg {
                        MempoolMsg::Add { item, key, reply_channel } => {
                            match pool.add_item(key, item) {
                                Ok(_id) => {
                                    if let Err(e) = reply_channel.send(Ok(())) {
                                        tracing::debug!("Failed to send reply to AddTx: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                   tracing::debug!("could not add tx to the pool due to: {}", e);
                                }
                            }
                        }
                        MempoolMsg::View { ancestor_hint, reply_channel } => {
                            reply_channel.send(pool.view(ancestor_hint)).unwrap_or_else(|_| {
                                tracing::debug!("could not send back pool view")
                            });
                        }
                        MempoolMsg::MarkInBlock { ids, block } => {
                            pool.mark_in_block(&ids, block);
                        }
                        #[cfg(test)]
                        MempoolMsg::BlockTransaction { block, reply_channel } => {
                            reply_channel.send(pool.block_items(block)).unwrap_or_else(|_| {
                                tracing::debug!("could not send back block items")
                            });
                        }
                        MempoolMsg::Prune { ids } => { pool.prune(&ids); },
                        MempoolMsg::Metrics { reply_channel } => {
                            let metrics = MempoolMetrics {
                                pending_items: pool.pending_item_count(),
                                last_item_timestamp: pool.last_item_timestamp(),
                            };
                            reply_channel.send(metrics).unwrap_or_else(|_| {
                                tracing::debug!("could not send back mempool metrics")
                            });
                        }
                    }
                }
                Some((key, item )) = network_items.next() => {
                    pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {}", e)
                    });
                }
            }
        }
    }
}

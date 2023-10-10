pub mod backend;
pub mod network;

// std
use std::{
    fmt::{Debug, Error, Formatter},
    marker::PhantomData,
};

// crates
use futures::StreamExt;
use tokio::sync::oneshot::Sender;
// internal
use crate::network::NetworkAdapter;
use backend::{MemPool, Status};
use nomos_core::block::BlockId;
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use tracing::error;

pub struct MempoolService<N, P, D>
where
    N: NetworkAdapter<Item = P::Item, Key = P::Key>,
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    D: Discriminant,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N::Backend>>,
    pool: P,
    // This is an hack because SERVICE_ID has to be univoque and associated const
    // values can't depend on generic parameters.
    // Unfortunately, this means that the mempools for certificates and transactions
    // would have the same SERVICE_ID and break overwatch asumptions.
    _d: PhantomData<D>,
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
    Status {
        items: Vec<Key>,
        reply_channel: Sender<Vec<Status>>,
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
            Self::BlockItems { block, .. } => {
                write!(f, "MempoolMsg::BlockItem{{block: {block:?}}}")
            }
            Self::Metrics { .. } => write!(f, "MempoolMsg::Metrics"),
            Self::Status { items, .. } => write!(f, "MempoolMsg::Status{{items: {items:?}}}"),
        }
    }
}

impl<Item: 'static, Key: 'static> RelayMessage for MempoolMsg<Item, Key> {}

pub struct Transaction;
pub struct Certificate;

pub trait Discriminant {
    const ID: &'static str;
}

impl Discriminant for Transaction {
    const ID: &'static str = "mempool-cl";
}

impl Discriminant for Certificate {
    const ID: &'static str = "mempool-da";
}

impl<N, P, D> ServiceData for MempoolService<N, P, D>
where
    N: NetworkAdapter<Item = P::Item, Key = P::Key>,
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    D: Discriminant,
{
    const SERVICE_ID: ServiceId = D::ID;
    type Settings = Settings<P::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MempoolMsg<<P as MemPool>::Item, <P as MemPool>::Key>;
}

#[async_trait::async_trait]
impl<N, P, D> ServiceCore for MempoolService<N, P, D>
where
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    N: NetworkAdapter<Item = P::Item, Key = P::Key> + Send + Sync + 'static,
    D: Discriminant + Send,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let settings = service_state.settings_reader.get_updated_settings();
        Ok(Self {
            service_state,
            network_relay,
            pool: P::new(settings.backend),
            _d: PhantomData,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            network_relay,
            mut pool,
            ..
        } = self;

        let mut network_relay: OutboundRelay<_> = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let adapter = N::new(
            service_state.settings_reader.get_updated_settings().network,
            network_relay.clone(),
        );
        let adapter = adapter.await;

        let mut network_items = adapter.transactions_stream().await;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();

        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    Self::handle_mempool_message(msg, &mut pool, &mut network_relay, &mut service_state).await;
                }
                Some((key, item )) = network_items.next() => {
                    pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {}", e)
                    });
                }
                Some(msg) = lifecycle_stream.next() =>  {
                    if Self::handle_lifecycle_message(msg).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<N, P, D> MempoolService<N, P, D>
where
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    N: NetworkAdapter<Item = P::Item, Key = P::Key> + Send + Sync + 'static,
    D: Discriminant + Send,
{
    async fn handle_lifecycle_message(message: LifecycleMessage) -> bool {
        match message {
            LifecycleMessage::Shutdown(sender) => {
                if sender.send(()).is_err() {
                    error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
            LifecycleMessage::Kill => true,
        }
    }

    async fn handle_mempool_message(
        message: MempoolMsg<P::Item, P::Key>,
        pool: &mut P,
        network_relay: &mut OutboundRelay<NetworkMsg<N::Backend>>,
        service_state: &mut ServiceStateHandle<Self>,
    ) {
        match message {
            MempoolMsg::Add {
                item,
                key,
                reply_channel,
            } => {
                match pool.add_item(key, item.clone()) {
                    Ok(_id) => {
                        // Broadcast the item to the network
                        let net = network_relay.clone();
                        let settings = service_state.settings_reader.get_updated_settings().network;
                        // move sending to a new task so local operations can complete in the meantime
                        tokio::spawn(async move {
                            let adapter = N::new(settings, net).await;
                            adapter.send(item).await;
                        });
                        if let Err(e) = reply_channel.send(Ok(())) {
                            tracing::debug!("Failed to send reply to AddTx: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("could not add tx to the pool due to: {}", e);
                    }
                }
            }
            MempoolMsg::View {
                ancestor_hint,
                reply_channel,
            } => {
                reply_channel
                    .send(pool.view(ancestor_hint))
                    .unwrap_or_else(|_| tracing::debug!("could not send back pool view"));
            }
            MempoolMsg::MarkInBlock { ids, block } => {
                pool.mark_in_block(&ids, block);
            }
            #[cfg(test)]
            MempoolMsg::BlockItems {
                block,
                reply_channel,
            } => {
                reply_channel
                    .send(pool.block_items(block))
                    .unwrap_or_else(|_| tracing::debug!("could not send back block items"));
            }
            MempoolMsg::Prune { ids } => {
                pool.prune(&ids);
            }
            MempoolMsg::Metrics { reply_channel } => {
                let metrics = MempoolMetrics {
                    pending_items: pool.pending_item_count(),
                    last_item_timestamp: pool.last_item_timestamp(),
                };
                reply_channel
                    .send(metrics)
                    .unwrap_or_else(|_| tracing::debug!("could not send back mempool metrics"));
            }
            MempoolMsg::Status {
                items,
                reply_channel,
            } => {
                reply_channel
                    .send(pool.status(&items))
                    .unwrap_or_else(|_| tracing::debug!("could not send back mempool status"));
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Settings<B, N> {
    pub backend: B,
    pub network: N,
}

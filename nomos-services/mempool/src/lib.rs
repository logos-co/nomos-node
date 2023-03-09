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
use nomos_core::block::{BlockHeader, BlockId};
use nomos_network::NetworkService;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

pub struct MempoolService<N, P>
where
    N: NetworkAdapter<Tx = P::Tx>,
    P: MemPool,
    P::Settings: Clone,
    P::Tx: Debug + 'static,
    P::Id: Debug + 'static,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N::Backend>>,
    pool: P,
}

pub struct MempoolMetrics {
    pub pending_txs: usize,
    pub last_tx_timestamp: u64,
}

pub enum MempoolMsg<Tx, Id> {
    AddTx {
        tx: Tx,
        reply_channel: Sender<Result<(), ()>>,
    },
    View {
        ancestor_hint: BlockId,
        reply_channel: Sender<Box<dyn Iterator<Item = Tx> + Send>>,
    },
    Prune {
        ids: Vec<Id>,
    },
    BlockTransaction {
        block: BlockId,
        reply_channel: Sender<Option<Box<dyn Iterator<Item = Tx> + Send>>>,
    },
    MarkInBlock {
        ids: Vec<Id>,
        block: BlockHeader,
    },
    Metrics {
        reply_channel: Sender<MempoolMetrics>,
    },
}

impl<Tx: Debug, Id: Debug> Debug for MempoolMsg<Tx, Id> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Self::View { ancestor_hint, .. } => {
                write!(f, "MempoolMsg::View {{ ancestor_hint: {ancestor_hint:?}}}")
            }
            Self::AddTx { tx, .. } => write!(f, "MempoolMsg::AddTx{{tx: {tx:?}}}"),
            Self::Prune { ids } => write!(f, "MempoolMsg::Prune{{ids: {ids:?}}}"),
            Self::MarkInBlock { ids, block } => {
                write!(
                    f,
                    "MempoolMsg::MarkInBlock{{ids: {ids:?}, block: {block:?}}}"
                )
            }
            Self::BlockTransaction { block, .. } => {
                write!(f, "MempoolMsg::BlockTransaction{{block: {block:?}}}")
            }
            Self::Metrics { .. } => write!(f, "MempoolMsg::Metrics"),
        }
    }
}

impl<Tx: 'static, Id: 'static> RelayMessage for MempoolMsg<Tx, Id> {}

impl<N, P> ServiceData for MempoolService<N, P>
where
    N: NetworkAdapter<Tx = P::Tx>,
    P: MemPool,
    P::Settings: Clone,
    P::Tx: Debug + 'static,
    P::Id: Debug + 'static,
{
    const SERVICE_ID: ServiceId = "Mempool";
    type Settings = P::Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MempoolMsg<<P as MemPool>::Tx, <P as MemPool>::Id>;
}

#[async_trait::async_trait]
impl<N, P> ServiceCore for MempoolService<N, P>
where
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    P::Id: Debug + Send + 'static,
    P::Tx: Clone + Debug + Send + Sync + 'static,
    N: NetworkAdapter<Tx = P::Tx> + Send + Sync + 'static,
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
                        MempoolMsg::AddTx { tx, reply_channel } => {
                            match pool.add_tx(tx.clone()) {
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
                        MempoolMsg::BlockTransaction { block, reply_channel } => {
                            reply_channel.send(pool.block_transactions(block)).unwrap_or_else(|_| {
                                tracing::debug!("could not send back block transactions")
                            });
                        }
                        MempoolMsg::Prune { ids } => { pool.prune(&ids); },
                        MempoolMsg::Metrics { reply_channel } => {
                            let metrics = MempoolMetrics {
                                pending_txs: pool.pending_tx_count(),
                                last_tx_timestamp: pool.last_tx_timestamp(),
                            };
                            reply_channel.send(metrics).unwrap_or_else(|_| {
                                tracing::debug!("could not send back mempool metrics")
                            });
                        }
                    }
                }
                Some(tx) = network_txs.next() => {
                    pool.add_tx(tx).unwrap_or_else(|e| {
                        tracing::debug!("could not add tx to the pool due to: {}", e)
                    });
                }
            }
        }
    }
}

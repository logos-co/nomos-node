/// std
use std::fmt::{Debug, Error, Formatter};

/// crates
use tokio::sync::broadcast::Receiver;
use tokio::sync::oneshot::Sender;

/// internal
pub mod backend;
use backend::Pool;
use nomos_core::block::{BlockHeader, BlockId};
use nomos_network::{
    backends::{waku::NetworkEvent, NetworkBackend},
    NetworkService,
};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

pub struct Mempool<
    N: NetworkBackend + Send + Sync + 'static,
    Tx: Debug + Send + Sync + 'static,
    Id: Debug + Send + Sync + 'static,
    P: Pool<Tx = Tx, Id = Id> + Send + Sync + 'static,
> {
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N>>,
    pool: P,
}

pub enum MempoolMsg<Tx, Id> {
    AddTx {
        id: Id,
        tx: Tx,
    },
    View {
        ancestor_hint: BlockId,
        rx: Sender<Box<dyn Iterator<Item = Tx> + Send>>,
    },
    Prune {
        ids: Vec<Id>,
    },
    MarkInBlock {
        ids: Vec<Id>,
        block: BlockHeader,
    },
}

impl<Tx: Debug, Id: Debug> Debug for MempoolMsg<Tx, Id> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            Self::View { ancestor_hint, .. } => {
                write!(
                    f,
                    "MempoolMsg::View {{ ancestor_hint: {:?}}}",
                    ancestor_hint
                )
            }
            Self::AddTx { id, tx } => write!(f, "MempoolMsg::AddTx{{id: {:?}, tx: {:?}}}", id, tx),
            Self::Prune { ids } => write!(f, "MempoolMsg::Prune{{ids: {:?}}}", ids),
            Self::MarkInBlock { ids, block } => {
                write!(
                    f,
                    "MempoolMsg::MarkInBlock{{ids: {:?}, block: {:?}}}",
                    ids, block
                )
            }
        }
    }
}

impl<Tx: 'static, Id: 'static> RelayMessage for MempoolMsg<Tx, Id> {}

impl<
        N: NetworkBackend + Send + Sync + 'static,
        Tx: Debug + Send + Sync + 'static,
        Id: Debug + Send + Sync + 'static,
        P: Pool<Tx = Tx, Id = Id> + Send + Sync + 'static,
    > ServiceData for Mempool<N, Tx, Id, P>
{
    const SERVICE_ID: ServiceId = "Mempool";
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MempoolMsg<Tx, Id>;
}

#[async_trait::async_trait]
impl<N, Tx, Id, P> ServiceCore for Mempool<N, Tx, Id, P>
where
    Tx: Debug + Send + Sync + 'static,
    Id: Debug + Send + Sync + 'static,
    P: Pool<Tx = Tx, Id = Id> + Send + Sync + 'static,
    N: NetworkBackend + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            network_relay,
            pool: P::new(),
        })
    }

    #[allow(unreachable_code, unused, clippy::diverging_sub_expression)]
    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            service_state,
            network_relay,
            pool,
        } = self;

        let network_relay: OutboundRelay<_> = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        // Separate function so that we can specialize it for different network backends
        let network_txs: Receiver<NetworkEvent> = todo!();

        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    match msg {
                        MempoolMsg::AddTx { id, tx } => {
                            pool.add_tx(tx, id).unwrap_or_else(|e| {
                                tracing::debug!("could not add tx to the pool due to: {}", e)
                            });
                        }
                        MempoolMsg::View { ancestor_hint, rx } => {
                            rx.send(pool.view(ancestor_hint)).unwrap_or_else(|_| {
                                tracing::debug!("could not send back pool view")
                            })
                        }
                        MempoolMsg::MarkInBlock { ids, block } => {
                            pool.mark_in_block(ids, block);
                        }
                        MempoolMsg::Prune { ids } => pool.prune(ids),
                    }
                }
                Ok(msg) = network_txs.recv() => {
                    // filter incoming transactions and add them to the pool
                    todo!()
                }
            }
        }
    }
}

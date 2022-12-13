//! In this module, and children ones, the 'view lifetime is tied to a logical consensus view,
//! represented by the `View` struct.
//! This is done to ensure that all the different data structs used to represent various actors
//! are always synchronized (i.e. it cannot happen that we accidentally use committees from different views).
//! It's obviously extremely important that the information contained in `View` is synchronized across different
//! nodes, but that has to be achieved through different means.
// mod backends;
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
use std::fmt::{Debug, Error, Formatter};
use tokio::sync::broadcast::Receiver;
use tokio::sync::oneshot::Sender;

pub type BlockId = [u8; 32];

pub struct Mempool<
    N: NetworkBackend + Send + Sync + 'static,
    Tx: Debug + Send + Sync + 'static,
    Id: Debug + Send + Sync + 'static,
    P: Pool<Tx = Tx, Id = Id> + Send + Sync + 'static,
> {
    service_state: ServiceStateHandle<Self>,
    // underlying networking backend. We need this so we can relay and check the types properly
    // when implementing ServiceCore for CarnotConsensus
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
        block: BlockId,
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

    #[allow(unreachable_code, unused)]
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

pub trait Pool {
    type Tx;
    type Id;

    /// Construct a new empty pool
    fn new() -> Self;

    /// Add a new transaction to the mempool, for example because we received it from the network
    fn add_tx(&mut self, tx: Self::Tx, id: Self::Id) -> Result<(), overwatch_rs::DynError>;

    /// Return a view over the transactions contained in the mempool.
    /// Implementations should provide *at least* all the transactions which have not been marked as
    /// in a block.
    /// The hint on the ancestor *can* be used by the implementation to display additional
    /// transactions that were not included up to that point if available.
    fn view(&self, ancestor_hint: BlockId) -> Box<dyn Iterator<Item = Self::Tx> + Send>;

    /// Record that a set of transactions were included in a block
    fn mark_in_block(&mut self, txs: Vec<Self::Id>, block: BlockId);

    /// Signal that a set of transactions can't be possibly requested anymore and can be
    /// discarded.
    fn prune(&mut self, txs: Vec<Self::Id>);
}

#[test]
fn test() {
    println!("{:?}", MempoolMsg::AddTx { id: 0, tx: 1 });
    assert!(false);
}

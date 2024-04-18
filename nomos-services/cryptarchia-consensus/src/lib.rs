mod leadership;
pub mod network;
mod time;

use core::fmt::Debug;
use cryptarchia_engine::Slot;
use cryptarchia_ledger::{Coin, LeaderProof, LedgerState};
use futures::StreamExt;
use network::{messages::NetworkMessage, NetworkAdapter};
use nomos_core::da::certificate::{BlobCertificateSelect, Certificate};
use nomos_core::header::{cryptarchia::Header, HeaderId};
use nomos_core::tx::{Transaction, TxSelect};
use nomos_core::{
    block::{builder::BlockBuilder, Block},
    header::cryptarchia::Builder,
};
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, TxMempoolService,
};
use nomos_mempool::{DaMempoolMsg, DaMempoolService, TxMempoolMsg};
use nomos_network::NetworkService;
use nomos_storage::{backends::StorageBackend, StorageMsg, StorageService};
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{OutboundRelay, Relay, RelayMessage};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::serde_as;
use std::hash::Hash;
use thiserror::Error;
pub use time::Config as TimeConfig;
use tokio::sync::oneshot::Sender;
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::IntervalStream;
use tracing::{error, instrument};

// Limit the number of blocks returned by GetHeaders
const HEADERS_LIMIT: usize = 512;

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("Ledger error: {0}")]
    Ledger(#[from] cryptarchia_ledger::LedgerError<HeaderId>),
    #[error("Consensus error: {0}")]
    Consensus(#[from] cryptarchia_engine::Error<HeaderId>),
}

struct Cryptarchia {
    ledger: cryptarchia_ledger::Ledger<HeaderId>,
    consensus: cryptarchia_engine::Cryptarchia<HeaderId>,
}

impl Cryptarchia {
    fn tip(&self) -> HeaderId {
        self.consensus.tip()
    }

    fn genesis(&self) -> HeaderId {
        self.consensus.genesis()
    }

    fn try_apply_header(&self, header: &Header) -> Result<Self, Error> {
        let id = header.id();
        let parent = header.parent();
        let slot = header.slot();
        let ledger = self.ledger.try_update(
            id,
            parent,
            slot,
            header.leader_proof(),
            header
                .orphaned_proofs()
                .iter()
                .map(|imported_header| (imported_header.id(), *imported_header.leader_proof())),
        )?;
        let consensus = self.consensus.receive_block(id, parent, slot)?;

        Ok(Self { ledger, consensus })
    }

    fn epoch_state_for_slot(&self, slot: Slot) -> Option<&cryptarchia_ledger::EpochState> {
        let tip = self.tip();
        let state = self.ledger.state(&tip).expect("no state for tip");
        let requested_epoch = self.ledger.config().epoch(slot);
        if state.epoch_state().epoch() == requested_epoch {
            Some(state.epoch_state())
        } else if requested_epoch == state.next_epoch_state().epoch() {
            Some(state.next_epoch_state())
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CryptarchiaSettings<Ts, Bs> {
    #[serde(default)]
    pub transaction_selector_settings: Ts,
    #[serde(default)]
    pub blob_selector_settings: Bs,
    pub config: cryptarchia_ledger::Config,
    pub genesis_state: LedgerState,
    pub time: TimeConfig,
    pub coins: Vec<Coin>,
}

impl<Ts, Bs> CryptarchiaSettings<Ts, Bs> {
    #[inline]
    pub const fn new(
        transaction_selector_settings: Ts,
        blob_selector_settings: Bs,
        config: cryptarchia_ledger::Config,
        genesis_state: LedgerState,
        time: TimeConfig,
        coins: Vec<Coin>,
    ) -> Self {
        Self {
            transaction_selector_settings,
            blob_selector_settings,
            config,
            genesis_state,
            time,
            coins,
        }
    }
}

pub struct CryptarchiaConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
where
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key>,

    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    Storage: StorageBackend + Send + Sync + 'static,
{
    service_state: ServiceStateHandle<Self>,
    // underlying networking backend. We need this so we can relay and check the types properly
    // when implementing ServiceCore for CryptarchiaConsensus
    network_relay: Relay<NetworkService<A::Backend>>,
    cl_mempool_relay: Relay<TxMempoolService<ClPoolAdapter, ClPool>>,
    da_mempool_relay: Relay<DaMempoolService<DaPoolAdapter, DaPool>>,
    block_subscription_sender: broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
    storage_relay: Relay<StorageService<Storage>>,
}

impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage> ServiceData
    for CryptarchiaConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
where
    A: NetworkAdapter,
    ClPool: MemPool<BlockId = HeaderId>,
    ClPool::Item: Clone + Eq + Hash + Debug,
    ClPool::Key: Debug,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPool::Item: Clone + Eq + Hash + Debug,
    DaPool::Key: Debug,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key>,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key>,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    Storage: StorageBackend + Send + Sync + 'static,
{
    const SERVICE_ID: ServiceId = "Cryptarchia";
    type Settings = CryptarchiaSettings<TxS::Settings, BS::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = ConsensusMsg<Block<ClPool::Item, DaPool::Item>>;
}

#[async_trait::async_trait]
impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage> ServiceCore
    for CryptarchiaConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
where
    A: NetworkAdapter<Tx = ClPool::Item, BlobCertificate = DaPool::Item>
        + Clone
        + Send
        + Sync
        + 'static,
    ClPool: MemPool<BlockId = HeaderId> + Send + Sync + 'static,
    ClPool::Settings: Send + Sync + 'static,
    DaPool: MemPool<BlockId = HeaderId> + Send + Sync + 'static,
    DaPool::Settings: Send + Sync + 'static,
    ClPool::Item: Transaction<Hash = ClPool::Key>
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
    DaPool::Item: Certificate<Hash = DaPool::Key>
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    ClPool::Key: Debug + Send + Sync,
    DaPool::Key: Debug + Send + Sync,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key> + Send + Sync + 'static,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key> + Send + Sync + 'static,
    TxS: TxSelect<Tx = ClPool::Item> + Clone + Send + Sync + 'static,
    TxS::Settings: Send + Sync + 'static,
    BS: BlobCertificateSelect<Certificate = DaPool::Item> + Clone + Send + Sync + 'static,
    BS::Settings: Send + Sync + 'static,
    Storage: StorageBackend + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let cl_mempool_relay = service_state.overwatch_handle.relay();
        let da_mempool_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        let (block_subscription_sender, _) = broadcast::channel(16);
        Ok(Self {
            service_state,
            network_relay,
            cl_mempool_relay,
            da_mempool_relay,
            block_subscription_sender,
            storage_relay,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let network_relay: OutboundRelay<_> = self
            .network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let cl_mempool_relay: OutboundRelay<_> = self
            .cl_mempool_relay
            .connect()
            .await
            .expect("Relay connection with MemPoolService should succeed");

        let da_mempool_relay: OutboundRelay<_> = self
            .da_mempool_relay
            .connect()
            .await
            .expect("Relay connection with MemPoolService should succeed");

        let storage_relay: OutboundRelay<_> = self
            .storage_relay
            .connect()
            .await
            .expect("Relay connection with StorageService should succeed");

        let CryptarchiaSettings {
            config,
            genesis_state,
            transaction_selector_settings,
            blob_selector_settings,
            time,
            coins,
        } = self.service_state.settings_reader.get_updated_settings();

        let genesis_id = HeaderId::from([0; 32]);
        let mut cryptarchia = Cryptarchia {
            consensus: <cryptarchia_engine::Cryptarchia<_>>::from_genesis(
                genesis_id,
                config.consensus_config.clone(),
            ),
            ledger: <cryptarchia_ledger::Ledger<_>>::from_genesis(
                genesis_id,
                genesis_state,
                config.clone(),
            ),
        };
        let adapter = A::new(network_relay).await;
        let tx_selector = TxS::new(transaction_selector_settings);
        let blob_selector = BS::new(blob_selector_settings);

        let mut incoming_blocks = adapter.blocks_stream().await;
        let mut leader = leadership::Leader::new(genesis_id, coins, config);
        let timer = time::Timer::new(time);

        let mut slot_timer = IntervalStream::new(timer.slot_interval());

        let mut lifecycle_stream = self.service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                    Some(block) = incoming_blocks.next() => {
                        cryptarchia = Self::process_block(
                            cryptarchia,
                            &mut leader,
                            block,
                            storage_relay.clone(),
                            cl_mempool_relay.clone(),
                            da_mempool_relay.clone(),
                            &mut self.block_subscription_sender
                        )
                        .await;
                    }

                    _ = slot_timer.next() => {
                        let slot = timer.current_slot();
                        let parent = cryptarchia.tip();
                        tracing::debug!("ticking for slot {}", u64::from(slot));

                        let Some(epoch_state) = cryptarchia.epoch_state_for_slot(slot) else {
                            tracing::error!("trying to propose a block for slot {} but epoch state is not available", u64::from(slot));
                            continue;
                        };
                        if let Some(proof) = leader.build_proof_for(epoch_state, slot, parent) {
                            tracing::debug!("proposing block...");
                            // TODO: spawn as a separate task?
                            let block = Self::propose_block(
                                parent,
                                proof,
                                tx_selector.clone(),
                                blob_selector.clone(),
                                cl_mempool_relay.clone(),
                                da_mempool_relay.clone(),
                            ).await;

                            if let Some(block) = block {
                                adapter.broadcast(NetworkMessage::Block(block)).await;
                            }
                        }
                    }

                    Some(msg) = self.service_state.inbound_relay.next() => {
                        Self::process_message(&cryptarchia, &self.block_subscription_sender, msg);
                    }
                    Some(msg) = lifecycle_stream.next() => {
                        if Self::should_stop_service(msg).await {
                            break;
                        }
                    }
            }
        }
        Ok(())
    }
}

impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
    CryptarchiaConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
where
    A: NetworkAdapter + Clone + Send + Sync + 'static,
    ClPool: MemPool<BlockId = HeaderId> + Send + Sync + 'static,
    ClPool::Settings: Send + Sync + 'static,
    DaPool: MemPool<BlockId = HeaderId> + Send + Sync + 'static,
    DaPool::Settings: Send + Sync + 'static,
    ClPool::Item: Transaction<Hash = ClPool::Key>
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
    DaPool::Item: Certificate<Hash = DaPool::Key>
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    TxS: TxSelect<Tx = ClPool::Item> + Clone + Send + Sync + 'static,
    BS: BlobCertificateSelect<Certificate = DaPool::Item> + Clone + Send + Sync + 'static,
    ClPool::Key: Debug + Send + Sync,
    DaPool::Key: Debug + Send + Sync,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key> + Send + Sync + 'static,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key> + Send + Sync + 'static,
    Storage: StorageBackend + Send + Sync + 'static,
{
    async fn should_stop_service(message: LifecycleMessage) -> bool {
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

    fn process_message(
        cryptarchia: &Cryptarchia,
        block_channel: &broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
        msg: ConsensusMsg<Block<ClPool::Item, DaPool::Item>>,
    ) {
        match msg {
            ConsensusMsg::Info { tx } => {
                let info = CryptarchiaInfo {
                    tip: cryptarchia.tip(),
                    slot: cryptarchia
                        .ledger
                        .state(&cryptarchia.tip())
                        .expect("tip state not available")
                        .slot(),
                    height: cryptarchia
                        .consensus
                        .branches()
                        .get(&cryptarchia.tip())
                        .expect("tip branch not available")
                        .length(),
                };
                tx.send(info).unwrap_or_else(|e| {
                    tracing::error!("Could not send consensus info through channel: {:?}", e)
                });
            }
            ConsensusMsg::BlockSubscribe { sender } => {
                sender.send(block_channel.subscribe()).unwrap_or_else(|_| {
                    tracing::error!("Could not subscribe to block subscription channel")
                });
            }
            ConsensusMsg::GetHeaders { from, to, tx } => {
                // default to tip block if not present
                let from = from.unwrap_or(cryptarchia.tip());
                // default to genesis block if not present
                let to = to.unwrap_or(cryptarchia.genesis());

                let mut res = Vec::new();
                let mut cur = from;

                let branches = cryptarchia.consensus.branches();
                while let Some(h) = branches.get(&cur) {
                    res.push(h.id());
                    // limit the response size
                    if cur == to || cur == cryptarchia.genesis() || res.len() >= HEADERS_LIMIT {
                        break;
                    }
                    cur = h.parent();
                }

                tx.send(res)
                    .unwrap_or_else(|_| tracing::error!("could not send blocks through channel"));
            }
        }
    }

    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    #[instrument(
        level = "debug",
        skip(cryptarchia, storage_relay, cl_mempool_relay, da_mempool_relay, leader)
    )]
    async fn process_block(
        mut cryptarchia: Cryptarchia,
        leader: &mut leadership::Leader,
        block: Block<ClPool::Item, DaPool::Item>,
        storage_relay: OutboundRelay<StorageMsg<Storage>>,
        cl_mempool_relay: OutboundRelay<TxMempoolMsg<HeaderId, ClPool::Item, ClPool::Key>>,
        da_mempool_relay: OutboundRelay<DaMempoolMsg<HeaderId, DaPool::Item, DaPool::Key>>,
        block_broadcaster: &mut broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
    ) -> Cryptarchia {
        tracing::debug!("received proposal {:?}", block);

        // TODO: filter on time?

        let header = block.header().cryptarchia();
        let id = header.id();
        match cryptarchia.try_apply_header(header) {
            Ok(new_state) => {
                // update leader
                leader.follow_chain(header.parent(), id, *header.leader_proof().commitment());

                // remove included content from mempool
                mark_in_cl_block(
                    cl_mempool_relay,
                    block.transactions().map(Transaction::hash),
                    id,
                )
                .await;

                mark_in_da_block(da_mempool_relay, block.blobs().map(Certificate::hash), id).await;

                // store block
                let msg = <StorageMsg<_>>::new_store_message(header.id(), block.clone());
                if let Err((e, _msg)) = storage_relay.send(msg).await {
                    tracing::error!("Could not send block to storage: {e}");
                }

                if let Err(e) = block_broadcaster.send(block) {
                    tracing::error!("Could not notify block to services {e}");
                }

                cryptarchia = new_state;
            }
            Err(Error::Consensus(cryptarchia_engine::Error::ParentMissing(parent))) => {
                tracing::debug!("missing parent {:?}", parent);
                // TODO: request parent block
            }
            Err(e) => tracing::debug!("invalid block {:?}: {e:?}", block),
        }

        cryptarchia
    }

    #[instrument(
        level = "debug",
        skip(cl_mempool_relay, da_mempool_relay, tx_selector, blob_selector)
    )]
    async fn propose_block(
        parent: HeaderId,
        proof: LeaderProof,
        tx_selector: TxS,
        blob_selector: BS,
        cl_mempool_relay: OutboundRelay<TxMempoolMsg<HeaderId, ClPool::Item, ClPool::Key>>,
        da_mempool_relay: OutboundRelay<DaMempoolMsg<HeaderId, DaPool::Item, DaPool::Key>>,
    ) -> Option<Block<ClPool::Item, DaPool::Item>> {
        let mut output = None;
        let cl_txs = get_cl_mempool_contents(cl_mempool_relay);
        let da_certs = get_da_mempool_contents(da_mempool_relay);

        match futures::join!(cl_txs, da_certs) {
            (Ok(cl_txs), Ok(da_certs)) => {
                let Ok(block) = BlockBuilder::new(tx_selector, blob_selector)
                    .with_cryptarchia_builder(Builder::new(parent, proof))
                    .with_transactions(cl_txs)
                    .with_blobs_certificates(da_certs)
                    .build()
                else {
                    panic!("Proposal block should always succeed to be built")
                };
                tracing::debug!("proposed block with id {:?}", block.header().id());
                output = Some(block);
            }
            (Err(_), _) => tracing::error!("Could not fetch block cl transactions"),
            (_, Err(_)) => tracing::error!("Could not fetch block da certificates"),
        }
        output
    }
}

#[derive(Debug)]
pub enum ConsensusMsg<Block> {
    Info {
        tx: Sender<CryptarchiaInfo>,
    },
    BlockSubscribe {
        sender: oneshot::Sender<broadcast::Receiver<Block>>,
    },
    GetHeaders {
        from: Option<HeaderId>,
        to: Option<HeaderId>,
        tx: Sender<Vec<HeaderId>>,
    },
}

impl<Block: 'static> RelayMessage for ConsensusMsg<Block> {}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CryptarchiaInfo {
    pub tip: HeaderId,
    pub slot: Slot,
    pub height: u64,
}

async fn get_cl_mempool_contents<Item, Key>(
    mempool: OutboundRelay<TxMempoolMsg<HeaderId, Item, Key>>,
) -> Result<Box<dyn Iterator<Item = Item> + Send>, tokio::sync::oneshot::error::RecvError> {
    let (reply_channel, rx) = tokio::sync::oneshot::channel();

    mempool
        .send(TxMempoolMsg::View {
            ancestor_hint: [0; 32].into(),
            reply_channel,
        })
        .await
        .unwrap_or_else(|(e, _)| eprintln!("Could not get transactions from mempool {e}"));

    rx.await
}

async fn get_da_mempool_contents<Item, Key>(
    mempool: OutboundRelay<DaMempoolMsg<HeaderId, Item, Key>>,
) -> Result<Box<dyn Iterator<Item = Item> + Send>, tokio::sync::oneshot::error::RecvError> {
    let (reply_channel, rx) = tokio::sync::oneshot::channel();

    mempool
        .send(DaMempoolMsg::View {
            ancestor_hint: [0; 32].into(),
            reply_channel,
        })
        .await
        .unwrap_or_else(|(e, _)| eprintln!("Could not get transactions from mempool {e}"));

    rx.await
}

async fn mark_in_cl_block<Item, Key>(
    mempool: OutboundRelay<TxMempoolMsg<HeaderId, Item, Key>>,
    ids: impl Iterator<Item = Key>,
    block: HeaderId,
) {
    mempool
        .send(TxMempoolMsg::MarkInBlock {
            ids: ids.collect(),
            block,
        })
        .await
        .unwrap_or_else(|(e, _)| tracing::error!("Could not mark items in block: {e}"))
}

async fn mark_in_da_block<Item, Key>(
    mempool: OutboundRelay<DaMempoolMsg<HeaderId, Item, Key>>,
    ids: impl Iterator<Item = Key>,
    block: HeaderId,
) {
    mempool
        .send(DaMempoolMsg::MarkInBlock {
            ids: ids.collect(),
            block,
        })
        .await
        .unwrap_or_else(|(e, _)| tracing::error!("Could not mark items in block: {e}"))
}

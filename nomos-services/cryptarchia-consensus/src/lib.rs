mod leadership;
pub mod network;
use core::fmt::Debug;
use futures::{Stream, StreamExt};
use network::{messages::BlockMsg, NetworkAdapter};
use nomos_core::block::Block;
use nomos_core::da::certificate::{BlobCertificateSelect, Certificate};
use nomos_core::tx::{Transaction, TxSelect};
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, Certificate as CertDiscriminant,
    MempoolMsg, MempoolService, Transaction as TxDiscriminant,
};
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
use tokio::sync::oneshot::Sender;
use tracing::{error, instrument};

use cryptarchia_engine::{
    ledger::LedgerState, Config as CryptarchiaConfig, Cryptarchia, Error as CryptarchiaError,
    Header, HeaderId, LeaderProof,
};

// Limit the number of blocks returned by GetBlocks
// Approx 64KB of data
const BLOCKS_LIMIT: usize = 512;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CryptarchiaSettings<Ts, Bs> {
    pub private_key: [u8; 32],
    #[serde(default)]
    pub transaction_selector_settings: Ts,
    #[serde(default)]
    pub blob_selector_settings: Bs,
    pub config: CryptarchiaConfig,
}

impl<Ts, Bs> CryptarchiaSettings<Ts, Bs> {
    #[inline]
    pub const fn new(
        private_key: [u8; 32],
        transaction_selector_settings: Ts,
        blob_selector_settings: Bs,
        config: CryptarchiaConfig,
    ) -> Self {
        Self {
            private_key,
            transaction_selector_settings,
            blob_selector_settings,
            config,
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

    ClPool::Item: Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Debug + 'static,
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
    cl_mempool_relay: Relay<MempoolService<ClPoolAdapter, ClPool, TxDiscriminant>>,
    da_mempool_relay: Relay<MempoolService<DaPoolAdapter, DaPool, CertDiscriminant>>,
    storage_relay: Relay<StorageService<Storage>>,
}

impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage> ServiceData
    for CryptarchiaConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
where
    A: NetworkAdapter,
    ClPool: MemPool<BlockId = HeaderId>,
    ClPool::Item: Debug,
    ClPool::Key: Debug,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPool::Item: Debug,
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
    type Message = ConsensusMsg;
}

#[async_trait::async_trait]
impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage> ServiceCore
    for CryptarchiaConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, Storage>
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
        Ok(Self {
            service_state,
            network_relay,
            cl_mempool_relay,
            da_mempool_relay,
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
            private_key,
            transaction_selector_settings,
            blob_selector_settings,
            config,
        } = self.service_state.settings_reader.get_updated_settings();

        let genesis = Header::new(
            [0; 32].into(),
            0,
            [0; 32].into(),
            0.into(),
            LeaderProof::new([0; 32].into(), [0; 32].into(), 0.into(), [0; 32].into()),
        );
        let mut cryptarchia = Cryptarchia::from_genesis(
            genesis,
            LedgerState::from_commitments([].into_iter()),
            config,
        );
        let adapter = A::new(network_relay).await;
        let tx_selector = TxS::new(transaction_selector_settings);
        let blob_selector = BS::new(blob_selector_settings);

        let incoming_blocks = adapter.blocks_stream().await;

        let genesis_header = cryptarchia.genesis();

        let mut lifecycle_stream = self.service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                    Some(block) = incoming_blocks.next() => {
                        cryptarchia = Self::process_block(
                            cryptarchia,
                            block,
                            storage_relay.clone(),
                            cl_mempool_relay.clone(),
                            da_mempool_relay.clone(),
                        )
                        .await
                    }

                    Some(msg) = self.service_state.inbound_relay.next() => {
                        Self::process_message(&cryptarchia, msg);
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

    fn process_message(cryptarchia: &Cryptarchia, msg: ConsensusMsg) {
        match msg {
            ConsensusMsg::Info { tx } => {
                let info = CryptarchiaInfo {
                    tip: cryptarchia.tip().clone(),
                };
                tx.send(info).unwrap_or_else(|e| {
                    tracing::error!("Could not send consensus info through channel: {:?}", e)
                });
            }
            ConsensusMsg::GetBlocks { from, to, tx } => {
                tx.send(Vec::new())
                    .unwrap_or_else(|_| tracing::error!("could not send blocks through channel"));
            }
        }
    }

    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    #[instrument(
        level = "debug",
        skip(cryptarchia, storage_relay, cl_mempool_relay, da_mempool_relay)
    )]
    async fn process_block(
        mut cryptarchia: Cryptarchia,
        block: BlockMsg,
        storage_relay: OutboundRelay<StorageMsg<Storage>>,
        cl_mempool_relay: OutboundRelay<MempoolMsg<HeaderId, ClPool::Item, ClPool::Key>>,
        da_mempool_relay: OutboundRelay<MempoolMsg<HeaderId, DaPool::Item, DaPool::Key>>,
    ) -> Cryptarchia {
        tracing::debug!("received proposal {:?}", block);

        let original_block = block;
        // ?let block = original_block.header().clone();

        // match cryptarchia.receive_block(block.clone()) {
        //     Ok(new_state) => {
        //         let msg = <StorageMsg<_>>::new_store_message(block.id, original_block.clone());
        //         if let Err((e, _msg)) = storage_relay.send(msg).await {
        //             tracing::error!("Could not send block to storage: {e}");
        //         }

        //         // // remove included content from mempool
        //         // mark_in_block(
        //         //     cl_mempool_relay,
        //         //     original_block.transactions().map(Transaction::hash),
        //         //     block.id,
        //         // )
        //         // .await;

        //         // mark_in_block(
        //         //     da_mempool_relay,
        //         //     original_block.blobs().map(Certificate::hash),
        //         //     block.id,
        //         // )
        //         // .await;
        //         cryptarchia = new_state;
        //     }
        //     Err(CryptarchiaError::ParentMissing(parent)) => {
        //         tracing::debug!("missing parent {:?}", parent);
        //         // TODO: request parent block
        //     }
        //     Err(e) => tracing::debug!("invalid block {:?}: {e:?}", block),
        // }

        cryptarchia
    }
}

#[derive(Debug)]
pub enum ConsensusMsg {
    Info {
        tx: Sender<CryptarchiaInfo>,
    },
    /// Walk the chain back from 'from' (the most recent block) to
    /// 'to' (the oldest block). If 'from' is None, the tip of the chain is used as a starting
    /// point. If 'to' is None or not known to the node, the genesis block is used as an end point.
    GetBlocks {
        from: Option<HeaderId>,
        to: Option<HeaderId>,
        tx: Sender<Vec<cryptarchia_engine::Block>>,
    },
}

impl RelayMessage for ConsensusMsg {}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CryptarchiaInfo {
    pub tip: cryptarchia_engine::Header,
}

async fn get_mempool_contents<Item, Key>(
    mempool: OutboundRelay<MempoolMsg<HeaderId, Item, Key>>,
) -> Result<Box<dyn Iterator<Item = Item> + Send>, tokio::sync::oneshot::error::RecvError> {
    let (reply_channel, rx) = tokio::sync::oneshot::channel();

    mempool
        .send(MempoolMsg::View {
            ancestor_hint: [0; 32].into(),
            reply_channel,
        })
        .await
        .unwrap_or_else(|(e, _)| eprintln!("Could not get transactions from mempool {e}"));

    rx.await
}

async fn mark_in_block<Item, Key>(
    mempool: OutboundRelay<MempoolMsg<HeaderId, Item, Key>>,
    ids: impl Iterator<Item = Key>,
    block: HeaderId,
) {
    mempool
        .send(MempoolMsg::MarkInBlock {
            ids: ids.collect(),
            block,
        })
        .await
        .unwrap_or_else(|(e, _)| tracing::error!("Could not mark items in block: {e}"))
}

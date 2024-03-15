mod leadership;
pub mod network;
use core::fmt::Debug;
use cryptarchia_ledger::LedgerState;
use futures::StreamExt;
use network::NetworkAdapter;
use nomos_core::block::Block;
use nomos_core::da::certificate::{BlobCertificateSelect, Certificate};
use nomos_core::header::{cryptarchia, HeaderId};
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
use thiserror::Error;
use tokio::sync::oneshot::Sender;
use tracing::{error, instrument};

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

    fn try_apply_header(&self, header: &cryptarchia::Header) -> Result<Self, Error> {
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
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CryptarchiaSettings<Ts, Bs> {
    #[serde(default)]
    pub transaction_selector_settings: Ts,
    #[serde(default)]
    pub blob_selector_settings: Bs,
    pub consensus_config: cryptarchia_engine::Config,
    pub ledger_config: cryptarchia_ledger::Config,
    pub genesis_state: LedgerState,
}

impl<Ts, Bs> CryptarchiaSettings<Ts, Bs> {
    #[inline]
    pub const fn new(
        transaction_selector_settings: Ts,
        blob_selector_settings: Bs,
        consensus_config: cryptarchia_engine::Config,
        ledger_config: cryptarchia_ledger::Config,
        genesis_state: LedgerState,
    ) -> Self {
        Self {
            transaction_selector_settings,
            blob_selector_settings,
            consensus_config,
            ledger_config,
            genesis_state,
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
            consensus_config,
            ledger_config,
            genesis_state,
            ..
        } = self.service_state.settings_reader.get_updated_settings();

        let genesis_id = HeaderId::from([0; 32]);
        let mut cryptarchia = Cryptarchia {
            ledger: <cryptarchia_ledger::Ledger<_>>::from_genesis(
                genesis_id,
                genesis_state,
                ledger_config,
            ),
            consensus: <cryptarchia_engine::Cryptarchia<_>>::from_genesis(
                genesis_id,
                consensus_config,
            ),
        };
        let adapter = A::new(network_relay).await;

        let mut incoming_blocks = adapter.blocks_stream().await;

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
                    tip: cryptarchia.tip(),
                };
                tx.send(info).unwrap_or_else(|e| {
                    tracing::error!("Could not send consensus info through channel: {:?}", e)
                });
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
        block: Block<ClPool::Item, DaPool::Item>,
        storage_relay: OutboundRelay<StorageMsg<Storage>>,
        cl_mempool_relay: OutboundRelay<MempoolMsg<HeaderId, ClPool::Item, ClPool::Key>>,
        da_mempool_relay: OutboundRelay<MempoolMsg<HeaderId, DaPool::Item, DaPool::Key>>,
    ) -> Cryptarchia {
        tracing::debug!("received proposal {:?}", block);

        let header = block.header();
        let id = header.id();
        match cryptarchia.try_apply_header(block.header().cryptarchia()) {
            Ok(new_state) => {
                // remove included content from mempool
                mark_in_block(
                    cl_mempool_relay,
                    block.transactions().map(Transaction::hash),
                    id,
                )
                .await;

                mark_in_block(da_mempool_relay, block.blobs().map(Certificate::hash), id).await;

                // store block
                let msg = <StorageMsg<_>>::new_store_message(header.id(), block);
                if let Err((e, _msg)) = storage_relay.send(msg).await {
                    tracing::error!("Could not send block to storage: {e}");
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
}

#[derive(Debug)]
pub enum ConsensusMsg {
    Info { tx: Sender<CryptarchiaInfo> },
}

impl RelayMessage for ConsensusMsg {}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CryptarchiaInfo {
    pub tip: HeaderId,
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

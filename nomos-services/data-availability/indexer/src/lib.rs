pub mod consensus;
pub mod indexer;
pub mod storage;

use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::sync::mpsc::Sender;

use consensus::ConsensusAdapter;
use cryptarchia_consensus::network::NetworkAdapter;
use cryptarchia_consensus::CryptarchiaConsensus;
use futures::StreamExt;
use indexer::DaIndexer;
use nomos_core::block::Block;
use nomos_core::da::certificate::{BlobCertificateSelect, Certificate};
use nomos_core::header::HeaderId;
use nomos_core::tx::{Transaction, TxSelect};
use nomos_mempool::verify::MempoolVerificationProvider;
use nomos_mempool::{backend::MemPool, network::NetworkAdapter as MempoolAdapter};
use nomos_storage::backends::StorageBackend;
use nomos_storage::StorageService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use storage::DaStorageAdapter;
use tracing::error;

pub type ConsensusRelay<
    A,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    DaVerificationProvider,
    TxS,
    BS,
    Storage,
> = Relay<
    CryptarchiaConsensus<
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        Storage,
    >,
>;

pub struct DataIndexerService<
    Indexer,
    DaStorage,
    Consensus,
    A,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    DaVerificationProvider,
    TxS,
    BS,
    ConsensusStorage,
> where
    Indexer: DaIndexer,
    Indexer::Blob: 'static,
    Indexer::VID: 'static,
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: Certificate + Into<DaPool::Item> + Debug,
    DaVerificationProvider: MempoolVerificationProvider<
        Payload = DaPoolAdapter::Payload,
        Parameters = <DaPoolAdapter::Payload as Certificate>::VerificationParameters,
    >,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    DaStorage: DaStorageAdapter,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    service_state: ServiceStateHandle<Self>,
    indexer: Indexer,
    storage_relay: Relay<StorageService<DaStorage::Backend>>,
    consensus_relay: ConsensusRelay<
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    >,
}

pub enum DaMsg<B, V> {
    AddIndex {
        vid: V,
    },
    GetRange {
        ids: Box<dyn Iterator<Item = V> + Send>,
        reply_channel: Sender<Vec<B>>,
    },
}

impl<B: 'static, V: 'static> Debug for DaMsg<B, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaMsg::AddIndex { .. } => {
                write!(f, "DaMsg::Index")
            }
            DaMsg::GetRange { .. } => {
                write!(f, "DaMsg::Get")
            }
        }
    }
}

impl<B: 'static, V: 'static> RelayMessage for DaMsg<B, V> {}

impl<
        Indexer,
        DaStorage,
        Consensus,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    > ServiceData
    for DataIndexerService<
        Indexer,
        DaStorage,
        Consensus,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Indexer: DaIndexer,
    Indexer::Blob: 'static,
    Indexer::VID: 'static,
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: Certificate + Into<DaPool::Item> + Debug,
    DaVerificationProvider: MempoolVerificationProvider<
        Payload = DaPoolAdapter::Payload,
        Parameters = <DaPoolAdapter::Payload as Certificate>::VerificationParameters,
    >,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    DaStorage: DaStorageAdapter,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    const SERVICE_ID: ServiceId = "DaStorage";
    type Settings = Settings<Indexer::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaMsg<Indexer::Blob, Indexer::VID>;
}

impl<
        Indexer,
        DaStorage,
        Consensus,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    >
    DataIndexerService<
        Indexer,
        DaStorage,
        Consensus,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Indexer: DaIndexer + Send + Sync,
    Indexer::Blob: 'static,
    Indexer::VID: 'static,
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: Certificate + Into<DaPool::Item> + Debug,
    DaVerificationProvider: MempoolVerificationProvider<
        Payload = DaPoolAdapter::Payload,
        Parameters = <DaPoolAdapter::Payload as Certificate>::VerificationParameters,
    >,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    DaStorage: DaStorageAdapter,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    async fn handle_new_block(
        indexer: &Indexer,
        consensus_adapter: &Consensus,
        storage_adapter: &DaStorage,
        block: Block<ClPool::Item, DaPool::Item>,
    ) -> Result<(), DynError> {
        todo!()
    }
    async fn handle_da_msg(
        indexer: &Indexer,
        msg: DaMsg<Indexer::Blob, Indexer::VID>,
    ) -> Result<(), DynError> {
        match msg {
            DaMsg::AddIndex { vid } => {
                todo!()
            }
            DaMsg::GetRange { ids, reply_channel } => {
                todo!()
            }
        }
        Ok(())
    }

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
}

#[async_trait::async_trait]
impl<
        Indexer,
        DaStorage,
        Consensus,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    > ServiceCore
    for DataIndexerService<
        Indexer,
        DaStorage,
        Consensus,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        DaVerificationProvider,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Indexer: DaIndexer + Send + Sync + 'static,
    Indexer::Settings: Clone + Send + Sync + 'static,
    Indexer::Blob: Debug + Send + Sync,
    Indexer::VID: Debug + Send + Sync,
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: Certificate + Into<DaPool::Item> + Debug,
    DaVerificationProvider: MempoolVerificationProvider<
        Payload = DaPoolAdapter::Payload,
        Parameters = <DaPoolAdapter::Payload as Certificate>::VerificationParameters,
    >,
    ClPool::Key: Debug + 'static,
    DaPool::Key: Debug + 'static,
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
    DaPool::Item: Certificate<Id = DaPool::Key>
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,

    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    DaStorage: DaStorageAdapter + Send + Sync + 'static,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    Consensus: ConsensusAdapter<Tx = ClPool::Item, Cert = DaPool::Item> + Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let indexer = Indexer::new(settings.indexer);
        let consensus_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            indexer,
            storage_relay,
            consensus_relay,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            indexer,
            consensus_relay,
            storage_relay,
        } = self;
        let consensus_relay = consensus_relay
            .connect()
            .await
            .expect("Relay connection with ConsensusService should succeed");
        let storage_relay = storage_relay
            .connect()
            .await
            .expect("Relay connection with StorageService should succeed");

        let consensus_adapter = Consensus::new(consensus_relay).await;
        let mut consensus_blocks = consensus_adapter.block_stream().await;
        let storage_adapter = DaStorage::new(storage_relay).await;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(block) = consensus_blocks.next() => {
                    if let Err(e) = Self::handle_new_block(&indexer, &consensus_adapter, &storage_adapter, block).await {
                        tracing::debug!("Failed to add  a new received block: {e:?}");
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    if let Err(e) = Self::handle_da_msg(&indexer, msg).await {
                        tracing::debug!("Failed to handle da msg: {e:?}");
                    }
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<B> {
    pub indexer: B,
}

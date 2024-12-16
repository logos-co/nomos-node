pub mod consensus;
pub mod storage;

use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::ops::Range;

use consensus::ConsensusAdapter;
use cryptarchia_consensus::network::NetworkAdapter;
use cryptarchia_consensus::CryptarchiaConsensus;
use futures::StreamExt;
use nomos_core::block::Block;
use nomos_core::da::blob::{info::DispersedBlobInfo, metadata::Metadata, BlobSelect};
use nomos_core::header::HeaderId;
use nomos_core::tx::{Transaction, TxSelect};
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_mempool::{backend::MemPool, network::NetworkAdapter as MempoolAdapter};
use nomos_storage::backends::StorageBackend;
use nomos_storage::StorageService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use rand::{RngCore, SeedableRng};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use storage::DaStorageAdapter;
use tokio::sync::oneshot::Sender;
use tracing::{error, span, Level};

const DA_INDEXER_TAG: ServiceId = "DA-Indexer";

pub type ConsensusRelay<
    A,
    BlendAdapter,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    TxS,
    BS,
    Storage,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
> = Relay<
    CryptarchiaConsensus<
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        Storage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >,
>;

pub struct DataIndexerService<
    B,
    DaStorage,
    Consensus,
    A,
    BlendAdapter,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    TxS,
    BS,
    ConsensusStorage,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
> where
    B: 'static,
    A: NetworkAdapter,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobSelect<BlobId = DaPool::Item>,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = B>,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    service_state: ServiceStateHandle<Self>,
    storage_relay: Relay<StorageService<DaStorage::Backend>>,
    #[allow(clippy::type_complexity)]
    consensus_relay: ConsensusRelay<
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >,
}

pub enum DaMsg<B, V: Metadata> {
    AddIndex {
        info: V,
    },
    GetRange {
        app_id: <V as Metadata>::AppId,
        range: Range<<V as Metadata>::Index>,
        reply_channel: Sender<Vec<(<V as Metadata>::Index, Vec<B>)>>,
    },
}

impl<B: 'static, V: Metadata + 'static> Debug for DaMsg<B, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaMsg::AddIndex { .. } => {
                write!(f, "DaMsg::AddIndex")
            }
            DaMsg::GetRange { .. } => {
                write!(f, "DaMsg::GetRange")
            }
        }
    }
}

impl<B: 'static, V: Metadata + 'static> RelayMessage for DaMsg<B, V> {}

impl<
        B,
        DaStorage,
        Consensus,
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    > ServiceData
    for DataIndexerService<
        B,
        DaStorage,
        Consensus,
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >
where
    B: 'static,
    A: NetworkAdapter,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobSelect<BlobId = DaPool::Item>,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = B>,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    const SERVICE_ID: ServiceId = DA_INDEXER_TAG;
    type Settings = IndexerSettings<DaStorage::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaMsg<B, DaPool::Item>;
}

impl<
        B,
        DaStorage,
        Consensus,
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >
    DataIndexerService<
        B,
        DaStorage,
        Consensus,
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >
where
    B: Send + Sync + 'static,
    A: NetworkAdapter,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    <DaPool::Item as Metadata>::Index: Send + Sync,
    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobSelect<BlobId = DaPool::Item>,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = B>,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    async fn handle_new_block(
        storage_adapter: &DaStorage,
        block: Block<ClPool::Item, DaPool::Item>,
    ) -> Result<(), DynError> {
        for info in block.blobs() {
            storage_adapter.add_index(info).await?;
        }
        Ok(())
    }

    async fn handle_da_msg(
        storage_adapter: &DaStorage,
        msg: DaMsg<B, DaPool::Item>,
    ) -> Result<(), DynError> {
        match msg {
            DaMsg::AddIndex { info } => storage_adapter.add_index(&info).await,
            DaMsg::GetRange {
                app_id,
                range,
                reply_channel,
            } => {
                let stream = storage_adapter.get_range_stream(app_id, range).await;
                let results = stream.collect::<Vec<_>>().await;

                reply_channel
                    .send(results)
                    .map_err(|_| "Error sending range response".into())
            }
        }
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
        B,
        DaStorage,
        Consensus,
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    > ServiceCore
    for DataIndexerService<
        B,
        DaStorage,
        Consensus,
        A,
        BlendAdapter,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >
where
    B: Debug + Send + Sync,
    A: NetworkAdapter,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
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
    DaPool::Item: DispersedBlobInfo<BlobId = DaPool::Key>
        + Metadata
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <DaPool::Item as Metadata>::AppId: Send + Sync,
    <DaPool::Item as Metadata>::Index: Send + Sync,

    A::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobSelect<BlobId = DaPool::Item>,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = B> + Send + Sync + 'static,
    DaStorage::Settings: Clone + Send + Sync + 'static,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    Consensus: ConsensusAdapter<Tx = ClPool::Item, Cert = DaPool::Item> + Send + Sync,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let consensus_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();

        Ok(Self {
            service_state,
            storage_relay,
            consensus_relay,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            consensus_relay,
            storage_relay,
        } = self;

        let trace_span = span!(Level::INFO, "service", service = DA_INDEXER_TAG);
        let _trace_guard = trace_span.enter();

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
        let storage_adapter = DaStorage::new(
            service_state.settings_reader.get_updated_settings().storage,
            storage_relay,
        )
        .await;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(block) = consensus_blocks.next() => {
                    if let Err(e) = Self::handle_new_block(&storage_adapter, block).await {
                        tracing::debug!("Failed to add  a new received block: {e:?}");
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    if let Err(e) = Self::handle_da_msg(&storage_adapter, msg).await {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerSettings<S> {
    pub storage: S,
}

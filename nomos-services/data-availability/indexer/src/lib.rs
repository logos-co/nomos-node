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
use nomos_tracing::info_with_id;
use nomos_utils::lifecycle;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use rand::{RngCore, SeedableRng};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use storage::DaStorageAdapter;
use tokio::sync::oneshot::Sender;
use tracing::instrument;

const DA_INDEXER_TAG: ServiceId = "DA-Indexer";

pub type ConsensusRelay<
    NetAdapter,
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
        NetAdapter,
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
    Blob,
    DaStorage,
    Consensus,
    NetAdapter,
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
    Blob: 'static,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    NetAdapter::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = Blob>,
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
        NetAdapter,
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

pub enum DaMsg<Blob, Meta: Metadata> {
    AddIndex {
        info: Meta,
    },
    GetRange {
        app_id: <Meta as Metadata>::AppId,
        range: Range<<Meta as Metadata>::Index>,
        reply_channel: Sender<Vec<(<Meta as Metadata>::Index, Vec<Blob>)>>,
    },
}

impl<Blob: 'static, Meta: Metadata + 'static> Debug for DaMsg<Blob, Meta> {
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

impl<Blob: 'static, Meta: Metadata + 'static> RelayMessage for DaMsg<Blob, Meta> {}

impl<
        Blob,
        DaStorage,
        Consensus,
        NetAdapter,
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
        Blob,
        DaStorage,
        Consensus,
        NetAdapter,
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
    Blob: 'static,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    NetAdapter::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = Blob>,
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
    type Message = DaMsg<Blob, DaPool::Item>;
}

impl<
        Blob,
        DaStorage,
        Consensus,
        NetAdapter,
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
        Blob,
        DaStorage,
        Consensus,
        NetAdapter,
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
    Blob: Send + Sync + 'static,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    DaPool::Item: DispersedBlobInfo + Metadata + Clone + Eq + Hash + Debug + 'static,
    <DaPool::Item as DispersedBlobInfo>::BlobId: AsRef<[u8]>,
    DaPool::Key: Debug + 'static,
    <DaPool::Item as Metadata>::Index: Send + Sync,
    NetAdapter::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = Blob>,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    #[instrument(skip_all)]
    async fn handle_new_block(
        storage_adapter: &DaStorage,
        block: Block<ClPool::Item, DaPool::Item>,
    ) -> Result<(), DynError> {
        for info in block.blobs() {
            info_with_id!(info.blob_id().as_ref(), "HandleNewBlock");
            storage_adapter.add_index(info).await?;
        }
        Ok(())
    }

    #[instrument(skip_all)]
    async fn handle_da_msg(
        storage_adapter: &DaStorage,
        msg: DaMsg<Blob, DaPool::Item>,
    ) -> Result<(), DynError> {
        match msg {
            DaMsg::AddIndex { info } => {
                info_with_id!(info.blob_id().as_ref(), "AddIndex");
                storage_adapter.add_index(&info).await
            }
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
}

#[async_trait::async_trait]
impl<
        Blob,
        DaStorage,
        Consensus,
        NetAdapter,
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
        Blob,
        DaStorage,
        Consensus,
        NetAdapter,
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
    Blob: Debug + Send + Sync,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
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
    <<DaPool as MemPool>::Item as DispersedBlobInfo>::BlobId: AsRef<[u8]>,
    NetAdapter::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = Blob> + Send + Sync + 'static,
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
    fn init(
        service_state: ServiceStateHandle<Self>,
        _init_state: Self::State,
    ) -> Result<Self, DynError> {
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
                    if lifecycle::should_stop_service::<Self>(&msg).await {
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

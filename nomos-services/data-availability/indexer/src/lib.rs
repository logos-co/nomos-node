pub mod consensus;
pub mod storage;

use std::{
    fmt::{Debug, Formatter},
    hash::Hash,
    ops::Range,
};

use consensus::ConsensusAdapter;
use cryptarchia_consensus::{network::NetworkAdapter, CryptarchiaConsensus};
use futures::StreamExt;
use nomos_core::{
    block::Block,
    da::blob::{info::DispersedBlobInfo, metadata::Metadata, BlobSelect},
    header::HeaderId,
    tx::{Transaction, TxSelect},
};
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_mempool::{
    backend::{MemPool, RecoverableMempool},
    network::NetworkAdapter as MempoolAdapter,
};
use nomos_storage::{backends::StorageBackend, StorageService};
use nomos_tracing::info_with_id;
use overwatch_rs::{
    services::{
        relay::{Relay, RelayMessage},
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    DynError, OpaqueServiceStateHandle,
};
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use services_utils::overwatch::lifecycle;
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
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
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
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
> where
    Blob: 'static,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: RecoverableMempool<BlockId = HeaderId>,
    ClPool::RecoveryState: Serialize + for<'de> Deserialize<'de>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    ClPool::Settings: Clone,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    DaPool::Settings: Clone,
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
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    service_state: OpaqueServiceStateHandle<Self>,
    storage_relay: Relay<StorageService<DaStorage::Backend>>,
    #[expect(clippy::allow_attributes_without_reason)]
    #[expect(clippy::type_complexity)]
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
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
            Self::AddIndex { .. } => {
                write!(f, "DaMsg::AddIndex")
            }
            Self::GetRange { .. } => {
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
    >
where
    Blob: 'static,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: RecoverableMempool<BlockId = HeaderId>,
    ClPool::RecoveryState: Serialize + for<'de> Deserialize<'de>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    ClPool::Settings: Clone,
    DaPool::Item: Metadata + Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    DaPool::Settings: Clone,
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
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    const SERVICE_ID: ServiceId = DA_INDEXER_TAG;
    type Settings = IndexerSettings<DaStorage::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State, Self::Settings>;
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
    >
where
    Blob: Send + Sync + 'static,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: RecoverableMempool<BlockId = HeaderId>,
    ClPool::RecoveryState: Serialize + for<'de> Deserialize<'de>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    ClPool::Settings: Clone,
    DaPool::Item: DispersedBlobInfo + Metadata + Clone + Eq + Hash + Debug + Sync + 'static,
    <DaPool::Item as DispersedBlobInfo>::BlobId: AsRef<[u8]>,
    DaPool::Key: Debug + 'static,
    DaPool::Settings: Clone,
    <DaPool::Item as Metadata>::Index: Send + Sync,
    NetAdapter::Backend: 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    DaStorage: DaStorageAdapter<Info = DaPool::Item, Blob = Blob> + Sync,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
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
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
        TimeBackend,
        ApiAdapter,
    >
where
    Blob: Debug + Send + Sync,
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: cryptarchia_consensus::blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    ClPool: RecoverableMempool<BlockId = HeaderId>,
    ClPool::RecoveryState: Serialize + for<'de> Deserialize<'de>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    ClPool::Key: Debug + 'static,
    ClPool::Settings: Clone,
    DaPool::Key: Debug + 'static,
    DaPool::Settings: Clone,
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
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    fn init(
        service_state: OpaqueServiceStateHandle<Self>,
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
        let storage_adapter = DaStorage::new(storage_relay).await;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        #[expect(
            clippy::redundant_pub_crate,
            reason = "Generated by `tokio::select` macro."
        )]
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
                    if lifecycle::should_stop_service::<Self>(&msg) {
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

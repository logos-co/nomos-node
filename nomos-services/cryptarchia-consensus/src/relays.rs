use std::hash::Hash;
// std
use std::fmt::Debug;
// Crates
use overwatch_rs::services::relay::{OutboundRelay, Relay};
use rand::{RngCore, SeedableRng};
use serde::de::DeserializeOwned;
// Internal
use crate::storage::adapters::StorageAdapter;
use crate::storage::StorageAdapter as StorageAdapterTrait;
use crate::{blend, network, MempoolRelay, SamplingRelay};
use nomos_blend_service::network::NetworkAdapter as BlendNetworkAdapter;
use nomos_blend_service::ServiceMessage;
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::blob::BlobSelect;
use nomos_core::header::HeaderId;
use nomos_core::tx::TxSelect;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_sampling::DaSamplingService;
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, DaMempoolService, TxMempoolService,
};
use nomos_network::{NetworkMsg, NetworkService};
use nomos_storage::backends::StorageBackend;
use nomos_storage::{StorageMsg, StorageService};

type NetworkRelay<NetworkBackend> = OutboundRelay<NetworkMsg<NetworkBackend>>;
type BlendRelay<BlendAdapterNetworkBroadcastSettings> =
    OutboundRelay<ServiceMessage<BlendAdapterNetworkBroadcastSettings>>;
type ClMempoolRelay<ClPool, ClPoolAdapter> = MempoolRelay<
    <ClPoolAdapter as MempoolAdapter>::Payload,
    <ClPool as MemPool>::Item,
    <ClPool as MemPool>::Key,
>;
type DaMempoolRelay<DaPool, DaPoolAdapter, SamplingBackendBlobId> = MempoolRelay<
    <DaPoolAdapter as MempoolAdapter>::Payload,
    <DaPool as MemPool>::Item,
    SamplingBackendBlobId,
>;
type StorageRelay<Storage> = OutboundRelay<StorageMsg<Storage>>;

pub struct CryptarchiaConsensusRelays<
    BlendAdapter,
    BS,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    NetworkAdapter,
    SamplingBackend,
    SamplingRng,
    Storage,
    TxS,
> where
    BlendAdapter: blend::BlendAdapter<Network: BlendNetworkAdapter>,
    BS: BlobSelect,
    ClPool: MemPool,
    ClPoolAdapter: MempoolAdapter,
    DaPool: MemPool,
    DaPoolAdapter: MempoolAdapter,
    NetworkAdapter: network::NetworkAdapter,
    Storage: StorageBackend + Send + Sync,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng>,
    TxS: TxSelect,
{
    network_relay: NetworkRelay<<NetworkAdapter as network::NetworkAdapter>::Backend>,
    blend_relay: BlendRelay<
        <<BlendAdapter as blend::BlendAdapter>::Network as BlendNetworkAdapter>::BroadcastSettings,
    >,
    cl_mempool_relay: ClMempoolRelay<ClPool, ClPoolAdapter>,
    da_mempool_relay: DaMempoolRelay<DaPool, DaPoolAdapter, SamplingBackend::BlobId>,
    storage_adapter: StorageAdapter<Storage, TxS::Tx, BS::BlobId>,
    sampling_relay: SamplingRelay<DaPool::Key>,
}

impl<
        BlendAdapter,
        BS,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        NetworkAdapter,
        SamplingBackend,
        SamplingRng,
        Storage,
        TxS,
    >
    CryptarchiaConsensusRelays<
        BlendAdapter,
        BS,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        NetworkAdapter,
        SamplingBackend,
        SamplingRng,
        Storage,
        TxS,
    >
where
    BlendAdapter: blend::BlendAdapter<Network: BlendNetworkAdapter>,
    BS: BlobSelect<BlobId = DaPool::Item> + Clone,
    ClPool: MemPool<BlockId = HeaderId>,
    ClPool::BlockId: Debug,
    ClPool::Item: Debug + DeserializeOwned + Eq + Hash + Clone + Send + Sync,
    ClPool::Key: Debug,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPool::BlockId: Debug,
    DaPool::Item: Debug + DeserializeOwned + Eq + Hash + Clone + Send + Sync,
    DaPool::Key: Debug,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    NetworkAdapter: network::NetworkAdapter,
    Storage: StorageBackend + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug,
    SamplingBackend::BlobId: Debug,
    TxS: TxSelect<Tx = ClPool::Item> + Clone + Send + Sync,
{
    pub async fn new(
        network_relay: NetworkRelay<<NetworkAdapter as network::NetworkAdapter>::Backend>,
        blend_relay: BlendRelay<
            <<BlendAdapter as blend::BlendAdapter>::Network as BlendNetworkAdapter>::BroadcastSettings,
        >,
        cl_mempool_relay: ClMempoolRelay<ClPool, ClPoolAdapter>,
        da_mempool_relay: DaMempoolRelay<DaPool, DaPoolAdapter, SamplingBackend::BlobId>,
        sampling_relay: SamplingRelay<DaPool::Key>,
        storage_relay: StorageRelay<Storage>,
    ) -> Self {
        let storage_adapter =
            StorageAdapter::<Storage, TxS::Tx, BS::BlobId>::new(storage_relay).await;
        Self {
            network_relay,
            blend_relay,
            cl_mempool_relay,
            da_mempool_relay,
            sampling_relay,
            storage_adapter,
        }
    }

    pub async fn from_relays<SamplingNetworkAdapter, SamplingStorage>(
        network_relay: Relay<NetworkService<NetworkAdapter::Backend>>,
        blend_relay: Relay<
            nomos_blend_service::BlendService<BlendAdapter::Backend, BlendAdapter::Network>,
        >,
        cl_mempool_relay: Relay<TxMempoolService<ClPoolAdapter, ClPool>>,
        da_mempool_relay: Relay<
            DaMempoolService<
                DaPoolAdapter,
                DaPool,
                SamplingBackend,
                SamplingNetworkAdapter,
                SamplingRng,
                SamplingStorage,
            >,
        >,
        sampling_relay: Relay<
            DaSamplingService<
                SamplingBackend,
                SamplingNetworkAdapter,
                SamplingRng,
                SamplingStorage,
            >,
        >,
        storage_relay: Relay<StorageService<Storage>>,
    ) -> Self
    where
        SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
        SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
    {
        let network_relay = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let blend_relay: OutboundRelay<_> = blend_relay
            .connect()
            .await
            .expect("Relay connection with nomos_blend_service::BlendService should succeed");

        let cl_mempool_relay: OutboundRelay<_> = cl_mempool_relay
            .connect()
            .await
            .expect("Relay connection with MemPoolService should succeed");

        let da_mempool_relay: OutboundRelay<_> = da_mempool_relay
            .connect()
            .await
            .expect("Relay connection with MemPoolService should succeed");

        let storage_relay: OutboundRelay<_> = storage_relay
            .connect()
            .await
            .expect("Relay connection with StorageService should succeed");

        let sampling_relay: OutboundRelay<_> = sampling_relay
            .connect()
            .await
            .expect("Relay connection with SamplingService should succeed");

        Self::new(
            network_relay,
            blend_relay,
            cl_mempool_relay,
            da_mempool_relay,
            sampling_relay,
            storage_relay,
        )
        .await
    }

    pub fn network_relay(
        &self,
    ) -> &NetworkRelay<<NetworkAdapter as network::NetworkAdapter>::Backend> {
        &self.network_relay
    }

    pub fn blend_relay(
        &self,
    ) -> &BlendRelay<<BlendAdapter::Network as BlendNetworkAdapter>::BroadcastSettings> {
        &self.blend_relay
    }

    pub fn cl_mempool_relay(&self) -> &ClMempoolRelay<ClPool, ClPoolAdapter> {
        &self.cl_mempool_relay
    }

    pub fn da_mempool_relay(
        &self,
    ) -> &DaMempoolRelay<DaPool, DaPoolAdapter, SamplingBackend::BlobId> {
        &self.da_mempool_relay
    }

    pub fn sampling_relay(&self) -> &SamplingRelay<DaPool::Key> {
        &self.sampling_relay
    }

    pub fn storage_adapter(&self) -> &StorageAdapter<Storage, TxS::Tx, BS::BlobId> {
        &self.storage_adapter
    }
}

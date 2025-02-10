use std::{fmt::Debug, hash::Hash, marker::PhantomData};

use nomos_blend_service::{network::NetworkAdapter as BlendNetworkAdapter, ServiceMessage};
use nomos_core::{
    da::blob::{info::DispersedBlobInfo, BlobSelect},
    header::HeaderId,
    tx::TxSelect,
};
use nomos_da_sampling::{backend::DaSamplingServiceBackend, DaSamplingService};
use nomos_mempool::{
    backend::{MemPool, RecoverableMempool},
    network::NetworkAdapter as MempoolAdapter,
    DaMempoolService, TxMempoolService,
};
use nomos_network::{NetworkMsg, NetworkService};
use nomos_storage::{backends::StorageBackend, StorageMsg, StorageService};
use overwatch::services::relay::{OutboundRelay, Relay};
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    blend, network,
    storage::{adapters::StorageAdapter, StorageAdapter as StorageAdapterTrait},
    MempoolRelay, SamplingRelay,
};

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
    DaVerifierBackend,
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
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend,
{
    network_relay: NetworkRelay<<NetworkAdapter as network::NetworkAdapter>::Backend>,
    blend_relay: BlendRelay<
        <<BlendAdapter as blend::BlendAdapter>::Network as BlendNetworkAdapter>::BroadcastSettings,
    >,
    cl_mempool_relay: ClMempoolRelay<ClPool, ClPoolAdapter>,
    da_mempool_relay: DaMempoolRelay<DaPool, DaPoolAdapter, SamplingBackend::BlobId>,
    storage_adapter: StorageAdapter<Storage, TxS::Tx, BS::BlobId>,
    sampling_relay: SamplingRelay<DaPool::Key>,
    _phantom_data: PhantomData<DaVerifierBackend>,
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
        DaVerifierBackend,
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
        DaVerifierBackend,
    >
where
    BlendAdapter: blend::BlendAdapter<Network: BlendNetworkAdapter>,
    BS: BlobSelect<BlobId = DaPool::Item>,
    ClPool: RecoverableMempool<BlockId = HeaderId>,
    ClPool::RecoveryState: Serialize + for<'de> Deserialize<'de>,
    ClPool::Item: Debug + DeserializeOwned + Eq + Hash + Clone + Send + Sync + 'static,
    ClPool::Key: 'static,
    ClPool::Settings: Clone,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    DaPool: RecoverableMempool<BlockId = HeaderId>,
    DaPool::BlockId: Debug,
    DaPool::Item: Debug + DeserializeOwned + Eq + Hash + Clone + Send + Sync + 'static,
    DaPool::Key: Debug + 'static,
    DaPool::RecoveryState: Serialize + for<'de> Deserialize<'de>,
    DaPool::Settings: Clone,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    NetworkAdapter: network::NetworkAdapter,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingRng: SeedableRng + RngCore,
    Storage: StorageBackend + Send + Sync + 'static,
    TxS: TxSelect<Tx = ClPool::Item>,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + Sync + 'static,
    DaVerifierBackend::Settings: Clone,
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
            _phantom_data: PhantomData,
        }
    }

    #[expect(clippy::allow_attributes_without_reason)]
    #[expect(clippy::type_complexity)]
    pub async fn from_relays<
        SamplingNetworkAdapter,
        SamplingStorage,
        DaVerifierNetwork,
        DaVerifierStorage,
        ApiAdapter,
    >(
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
                DaVerifierBackend,
                DaVerifierNetwork,
                DaVerifierStorage,
                ApiAdapter,
            >,
        >,
        sampling_relay: Relay<
            DaSamplingService<
                SamplingBackend,
                SamplingNetworkAdapter,
                SamplingRng,
                SamplingStorage,
                DaVerifierBackend,
                DaVerifierNetwork,
                DaVerifierStorage,
                ApiAdapter,
            >,
        >,
        storage_relay: Relay<StorageService<Storage>>,
    ) -> Self
    where
        SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
        SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
        DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter + Send + Sync,
        DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + Sync + 'static,
        DaVerifierBackend::Settings: Clone,
        DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter + Send + Sync,
        DaVerifierNetwork::Settings: Clone,
        ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
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

    pub const fn network_relay(
        &self,
    ) -> &NetworkRelay<<NetworkAdapter as network::NetworkAdapter>::Backend> {
        &self.network_relay
    }

    pub const fn blend_relay(
        &self,
    ) -> &BlendRelay<<BlendAdapter::Network as BlendNetworkAdapter>::BroadcastSettings> {
        &self.blend_relay
    }

    pub const fn cl_mempool_relay(&self) -> &ClMempoolRelay<ClPool, ClPoolAdapter> {
        &self.cl_mempool_relay
    }

    pub const fn da_mempool_relay(
        &self,
    ) -> &DaMempoolRelay<DaPool, DaPoolAdapter, SamplingBackend::BlobId> {
        &self.da_mempool_relay
    }

    pub const fn sampling_relay(&self) -> &SamplingRelay<DaPool::Key> {
        &self.sampling_relay
    }

    pub const fn storage_adapter(&self) -> &StorageAdapter<Storage, TxS::Tx, BS::BlobId> {
        &self.storage_adapter
    }
}

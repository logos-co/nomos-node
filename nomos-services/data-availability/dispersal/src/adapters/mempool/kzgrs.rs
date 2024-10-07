// std
use std::hash::Hash;
use std::{fmt::Debug, marker::PhantomData};
// crates
use rand::{RngCore, SeedableRng};
use tokio::sync::oneshot;
// internal
use super::DaMempoolAdapter;
use kzgrs_backend::dispersal::{self, BlobInfo};
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::BlobId;
use nomos_core::header::HeaderId;
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, DaMempoolService, MempoolMsg,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};

type MempoolRelay<Payload, Item, Key> = OutboundRelay<MempoolMsg<HeaderId, Payload, Item, Key>>;

pub struct KzgrsMempoolAdapter<
    DaPoolAdapter,
    DaPool,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
> where
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    DaPool::Item: Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
{
    pub mempool_relay: MempoolRelay<DaPoolAdapter::Payload, DaPool::Item, DaPool::Key>,
    _phantom: PhantomData<(
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    )>,
}

#[async_trait::async_trait]
impl<
        DaPoolAdapter,
        DaPool,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    > DaMempoolAdapter
    for KzgrsMempoolAdapter<
        DaPoolAdapter,
        DaPool,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >
where
    DaPool: MemPool<BlockId = HeaderId, Key = BlobId>,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key, Payload = BlobInfo>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug + Send,
    DaPool::Item: Clone + Eq + Hash + Debug + Send + 'static,
    DaPool::Key: Debug + Send + 'static,
    SamplingRng: SeedableRng + RngCore + Sync,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send + Sync,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter + Send + Sync,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter + Send + Sync,
{
    type MempoolService = DaMempoolService<
        DaPoolAdapter,
        DaPool,
        SamplingBackend,
        SamplingNetworkAdapter,
        SamplingRng,
        SamplingStorage,
    >;
    type BlobId = BlobId;
    type Metadata = dispersal::Metadata;

    fn new(mempool_relay: OutboundRelay<<Self::MempoolService as ServiceData>::Message>) -> Self {
        Self {
            mempool_relay,
            _phantom: Default::default(),
        }
    }

    async fn post_blob_id(
        &self,
        blob_id: Self::BlobId,
        metadata: Self::Metadata,
    ) -> Result<(), DynError> {
        let (reply_channel, receiver) = oneshot::channel();
        self.mempool_relay
            .send(MempoolMsg::Add {
                payload: BlobInfo::new(blob_id, metadata),
                key: blob_id,
                reply_channel,
            })
            .await
            .map_err(|(e, _)| Box::new(e) as DynError)?;

        receiver
            .await
            .map_err(|e| Box::new(e) as DynError)?
            .map_err(|_| "Failed to receive response from the mempool".into())
    }
}

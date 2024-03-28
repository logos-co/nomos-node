pub mod backend;

use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::sync::mpsc::Sender;

use backend::DaStorageBackend;
use cryptarchia_consensus::network::NetworkAdapter;
use cryptarchia_consensus::CryptarchiaConsensus;
use futures::StreamExt;
use nomos_core::da::certificate::{BlobCertificateSelect, Certificate};
use nomos_core::header::HeaderId;
use nomos_core::tx::{Transaction, TxSelect};
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
use tracing::error;

pub type ConsensusRelay<
    A,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    TxS,
    BS,
    ConsensusStorage,
> = Relay<
    CryptarchiaConsensus<
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >,
>;

pub struct DataIndexerService<
    Backend,
    Storage,
    A,
    ClPool,
    ClPoolAdapter,
    DaPool,
    DaPoolAdapter,
    TxS,
    BS,
    ConsensusStorage,
> where
    Backend: DaStorageBackend,
    Backend::Blob: 'static,
    Backend::VID: 'static,
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
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    service_state: ServiceStateHandle<Self>,
    backend: Backend,
    storage_relay: Relay<StorageService<Storage>>,
    consensus_relay:
        ConsensusRelay<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, TxS, BS, ConsensusStorage>,
}

impl<
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >
    DataIndexerService<
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Backend: DaStorageBackend,
    Backend::Blob: 'static,
    Backend::VID: 'static,
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
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
}

pub enum DaMsg<B, V> {
    // Index blob to indexed application data.
    //
    // TODO: naming - Store is used by the verifier and the indexer services. Verifier adds
    // verified blobs, indexer tracks the blockchain and promotes blobs to be available via the
    // api.
    AddIndex {
        vid: V,
    },
    GetRange {
        ids: Box<dyn Iterator<Item = B> + Send>,
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
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    > ServiceData
    for DataIndexerService<
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Backend: DaStorageBackend,
    Backend::Blob: 'static,
    Backend::VID: 'static,
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
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    const SERVICE_ID: ServiceId = "DaStorage";
    type Settings = Settings<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaMsg<Backend::Blob, Backend::VID>;
}

impl<
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >
    DataIndexerService<
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Backend: DaStorageBackend + Send + Sync,
    Backend::Blob: 'static,
    Backend::VID: 'static,
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
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    async fn handle_da_msg(
        backend: &Backend,
        msg: DaMsg<Backend::Blob, Backend::VID>,
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
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    > ServiceCore
    for DataIndexerService<
        Backend,
        Storage,
        A,
        ClPool,
        ClPoolAdapter,
        DaPool,
        DaPoolAdapter,
        TxS,
        BS,
        ConsensusStorage,
    >
where
    Backend: DaStorageBackend + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Backend::Blob: Debug + Send + Sync,
    Backend::VID: Debug + Send + Sync,
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool<BlockId = HeaderId>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key>,
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
    Storage: StorageBackend + Send + Sync + 'static,
    ConsensusStorage: StorageBackend + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let backend = Backend::new(settings.backend);
        let consensus_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            service_state,
            backend,
            storage_relay,
            consensus_relay,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            backend,
            consensus_relay,
            storage_relay,
        } = self;
        let consensus_relay = consensus_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");
        let storage_relay = storage_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    todo!()
                }
                Some(msg) = lifecycle_stream.next() => {
                    todo!()
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings<B> {
    pub backend: B,
}

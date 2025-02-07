pub mod blend;
mod leadership;
mod messages;
pub mod network;
mod relays;
pub mod storage;
mod time;

use crate::relays::CryptarchiaConsensusRelays;
use crate::storage::adapters::StorageAdapter;
use crate::storage::StorageAdapter as _;
use core::fmt::Debug;
use cryptarchia_engine::Slot;
use futures::StreamExt;
pub use leadership::LeaderConfig;
use network::NetworkAdapter;
use nomos_core::da::blob::{
    info::DispersedBlobInfo, metadata::Metadata as BlobMetadata, BlobSelect,
};
use nomos_core::{
    block::{builder::BlockBuilder, Block},
    header::{Builder, Header, HeaderId},
    proofs::leader_proof::Risc0LeaderProof,
    tx::{Transaction, TxSelect},
};
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_sampling::{DaSamplingService, DaSamplingServiceMsg};
use nomos_ledger::{leader_proof::LeaderProof, LedgerState};
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, DaMempoolService, MempoolMsg,
    TxMempoolService,
};
use nomos_network::NetworkService;
use nomos_storage::{backends::StorageBackend, StorageMsg, StorageService};
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{OutboundRelay, Relay, RelayMessage};
use overwatch_rs::services::state::ServiceState;
use overwatch_rs::services::{handle::ServiceStateHandle, ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::marker::PhantomData;
use std::path::PathBuf;
use thiserror::Error;
pub use time::Config as TimeConfig;
use tokio::sync::oneshot::Sender;
use tokio::sync::{broadcast, oneshot};
use tokio_stream::wrappers::IntervalStream;
use tracing::{error, instrument, span, Level};
use tracing_futures::Instrument;
use utils::overwatch::recovery::backends::FileBackendSettings;
use utils::overwatch::{JsonFileBackend, RecoveryOperator};

type MempoolRelay<Payload, Item, Key> = OutboundRelay<MempoolMsg<HeaderId, Payload, Item, Key>>;
type SamplingRelay<BlobId> = OutboundRelay<DaSamplingServiceMsg<BlobId>>;

// Limit the number of blocks returned by GetHeaders
const HEADERS_LIMIT: usize = 512;
const CRYPTARCHIA_ID: ServiceId = "Cryptarchia";

#[derive(Debug, Clone, Error)]
pub enum Error {
    #[error("Ledger error: {0}")]
    Ledger(#[from] nomos_ledger::LedgerError<HeaderId>),
    #[error("Consensus error: {0}")]
    Consensus(#[from] cryptarchia_engine::Error<HeaderId>),
}

struct Cryptarchia {
    ledger: nomos_ledger::Ledger<HeaderId>,
    consensus: cryptarchia_engine::Cryptarchia<HeaderId>,
}

impl Cryptarchia {
    fn tip(&self) -> HeaderId {
        self.consensus.tip()
    }

    fn tip_state(&self) -> &LedgerState {
        self.ledger
            .state(&self.tip())
            .expect("tip state not available")
    }

    fn genesis(&self) -> HeaderId {
        self.consensus.genesis()
    }

    fn try_apply_header(&self, header: &Header) -> Result<Self, Error> {
        let id = header.id();
        let parent = header.parent();
        let slot = header.slot();
        let ledger = self.ledger.try_update(
            id,
            parent,
            slot,
            header.leader_proof(),
            header.orphaned_proofs().iter().map(|imported_header| {
                (
                    imported_header.id(),
                    imported_header.leader_proof().to_orphan_proof(),
                )
            }),
        )?;
        let consensus = self.consensus.receive_block(id, parent, slot)?;

        Ok(Self { ledger, consensus })
    }

    fn epoch_state_for_slot(&self, slot: Slot) -> Option<&nomos_ledger::EpochState> {
        let tip = self.tip();
        let state = self.ledger.state(&tip).expect("no state for tip");
        let requested_epoch = self.ledger.config().epoch(slot);
        if state.epoch_state().epoch() == requested_epoch {
            Some(state.epoch_state())
        } else if requested_epoch == state.next_epoch_state().epoch() {
            Some(state.next_epoch_state())
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CryptarchiaSettings<Ts, Bs, NetworkAdapterSettings, BlendAdapterSettings> {
    #[serde(default)]
    pub transaction_selector_settings: Ts,
    #[serde(default)]
    pub blob_selector_settings: Bs,
    pub config: nomos_ledger::Config,
    pub genesis_state: LedgerState,
    pub time: TimeConfig,
    pub leader_config: LeaderConfig,
    pub network_adapter_settings: NetworkAdapterSettings,
    pub blend_adapter_settings: BlendAdapterSettings,
    pub recovery_file: PathBuf,
}

impl<Ts, Bs, NetworkAdapterSettings, BlendAdapterSettings> FileBackendSettings
    for CryptarchiaSettings<Ts, Bs, NetworkAdapterSettings, BlendAdapterSettings>
{
    fn recovery_file(&self) -> &PathBuf {
        &self.recovery_file
    }
}

pub struct CryptarchiaConsensus<
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
> where
    NetAdapter: NetworkAdapter,
    NetAdapter::Backend: 'static,
    NetAdapter::Settings: Send,
    BlendAdapter: blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPool: MemPool<BlockId = HeaderId>,
    ClPool::Item: Clone + Eq + Hash + Debug + 'static,
    ClPool::Key: Debug + 'static,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPool::Item: Clone + Eq + Hash + Debug + 'static,
    DaPool::Key: Debug + 'static,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    Storage: StorageBackend + Send + Sync + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingRng: SeedableRng + RngCore,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    service_state: ServiceStateHandle<Self>,
    // underlying networking backend. We need this so we can relay and check the types properly
    // when implementing ServiceCore for CryptarchiaConsensus
    network_relay: Relay<NetworkService<NetAdapter::Backend>>,
    blend_relay:
        Relay<nomos_blend_service::BlendService<BlendAdapter::Backend, BlendAdapter::Network>>,
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
        DaSamplingService<SamplingBackend, SamplingNetworkAdapter, SamplingRng, SamplingStorage>,
    >,
    block_subscription_sender: broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
    storage_relay: Relay<StorageService<Storage>>,
    initial_state: <Self as ServiceData>::State,
}

impl<
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
    > ServiceData
    for CryptarchiaConsensus<
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
    >
where
    NetAdapter: NetworkAdapter,
    NetAdapter::Settings: Send,
    BlendAdapter: blend::BlendAdapter,
    BlendAdapter::Settings: Send,
    ClPool: MemPool<BlockId = HeaderId>,
    ClPool::Item: Clone + Eq + Hash + Debug,
    ClPool::Key: Debug,
    ClPoolAdapter: MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key>,
    DaPool: MemPool<BlockId = HeaderId>,
    DaPool::Item: Clone + Eq + Hash + Debug,
    DaPool::Key: Debug,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key>,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    TxS: TxSelect<Tx = ClPool::Item>,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item>,
    BS::Settings: Send,
    Storage: StorageBackend + Send + Sync + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = DaPool::Key> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingRng: SeedableRng + RngCore,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    const SERVICE_ID: ServiceId = CRYPTARCHIA_ID;
    type Settings = CryptarchiaSettings<
        TxS::Settings,
        BS::Settings,
        NetAdapter::Settings,
        BlendAdapter::Settings,
    >;
    type State = CryptarchiaConsensusState<
        TxS::Settings,
        BS::Settings,
        NetAdapter::Settings,
        BlendAdapter::Settings,
    >;
    type StateOperator = RecoveryOperator<JsonFileBackend<Self::State, Self::Settings>>;
    type Message = ConsensusMsg<Block<ClPool::Item, DaPool::Item>>;
}

#[async_trait::async_trait]
impl<
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
    > ServiceCore
    for CryptarchiaConsensus<
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
    >
where
    NetAdapter: NetworkAdapter<Tx = ClPool::Item, BlobCertificate = DaPool::Item>
        + Clone
        + Send
        + Sync
        + 'static,
    NetAdapter::Settings: Send + Sync + 'static,
    BlendAdapter: blend::BlendAdapter<Tx = ClPool::Item, BlobCertificate = DaPool::Item>
        + Clone
        + Send
        + Sync
        + 'static,
    BlendAdapter::Settings: Send + Sync + 'static,
    ClPool: MemPool<BlockId = HeaderId> + Send + Sync + 'static,
    ClPool::Settings: Send + Sync + 'static,
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
    ClPool::Key: Debug + Send + Sync,
    ClPoolAdapter:
        MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key> + Send + Sync + 'static,
    DaPool: MemPool<BlockId = HeaderId, Key = SamplingBackend::BlobId> + Send + Sync + 'static,
    DaPool::Settings: Send + Sync + 'static,
    // TODO: Change to specific certificate bounds here
    DaPool::Item: DispersedBlobInfo<BlobId = DaPool::Key>
        + BlobMetadata
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key> + Send + Sync + 'static,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    TxS: TxSelect<Tx = ClPool::Item> + Clone + Send + Sync + 'static,
    TxS::Settings: Send + Sync + 'static,
    BS: BlobSelect<BlobId = DaPool::Item> + Clone + Send + Sync + 'static,
    BS::Settings: Send + Sync + 'static,
    Storage: StorageBackend + Send + Sync + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + Send + 'static,
    SamplingBackend::BlobId: Debug + Ord + Send + Sync + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingRng: SeedableRng + RngCore,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    fn init(
        service_state: ServiceStateHandle<Self>,
        initial_state: Self::State,
    ) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let blend_relay = service_state.overwatch_handle.relay();
        let cl_mempool_relay = service_state.overwatch_handle.relay();
        let da_mempool_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        let sampling_relay = service_state.overwatch_handle.relay();
        let (block_subscription_sender, _) = broadcast::channel(16);

        Ok(Self {
            service_state,
            network_relay,
            blend_relay,
            cl_mempool_relay,
            da_mempool_relay,
            block_subscription_sender,
            storage_relay,
            sampling_relay,
            initial_state,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let relays: CryptarchiaConsensusRelays<
            BlendAdapter,
            BS,
            ClPool,
            ClPoolAdapter,
            DaPool,
            DaPoolAdapter,
            NetAdapter,
            SamplingBackend,
            SamplingRng,
            Storage,
            TxS,
        > = CryptarchiaConsensusRelays::from_relays(
            self.network_relay,
            self.blend_relay,
            self.cl_mempool_relay,
            self.da_mempool_relay,
            self.sampling_relay,
            self.storage_relay,
        )
        .await;

        let CryptarchiaSettings {
            config,
            genesis_state,
            transaction_selector_settings,
            blob_selector_settings,
            time,
            leader_config,
            network_adapter_settings,
            blend_adapter_settings,
            ..
        } = self.service_state.settings_reader.get_updated_settings();

        let genesis_id = HeaderId::from([0; 32]);
        let mut cryptarchia = Cryptarchia {
            consensus: <cryptarchia_engine::Cryptarchia<_>>::from_genesis(
                genesis_id,
                config.consensus_config.clone(),
            ),
            ledger: <nomos_ledger::Ledger<_>>::from_genesis(
                genesis_id,
                genesis_state,
                config.clone(),
            ),
        };
        let mut leader = leadership::Leader::new(genesis_id, leader_config, config);

        if self.initial_state.should_recover() {
            if !self.initial_state.can_recover() {
                return Err(DynError::from(
                    "Security block is missing from recovery state.",
                ));
            }

            let CryptarchiaConsensusState {
                tip,
                security_block,
                ..
            } = self.initial_state;

            cryptarchia = Self::restore_from_recovery(
                cryptarchia,
                tip.unwrap(),
                security_block.unwrap(),
                &mut leader,
                &relays,
                &mut self.block_subscription_sender,
            )
            .await;
        }

        let network_adapter =
            NetAdapter::new(network_adapter_settings, relays.network_relay().clone()).await;
        let tx_selector = TxS::new(transaction_selector_settings);
        let blob_selector = BS::new(blob_selector_settings);

        let mut incoming_blocks = network_adapter.blocks_stream().await?;
        let timer = time::Timer::new(time);

        let mut slot_timer = IntervalStream::new(timer.slot_interval());

        let blend_adapter =
            BlendAdapter::new(blend_adapter_settings, relays.blend_relay().clone()).await;

        let mut lifecycle_stream = self.service_state.lifecycle_handle.message_stream();

        async {
            loop {
                tokio::select! {
                    Some(block) = incoming_blocks.next() => {
                        Self::log_received_block(&block);
                        cryptarchia = Self::process_block(
                            cryptarchia,
                            &mut leader,
                            block,
                            &relays,
                            &mut self.block_subscription_sender,
                        )
                        .await;

                        self.service_state.state_updater.update({
                            Self::State::new(
                                Some(cryptarchia.tip()),
                                cryptarchia.consensus.get_security_block_header_id()
                            )
                        });

                        tracing::info!(counter.consensus_processed_blocks = 1);
                    }

                    _ = slot_timer.next() => {
                        let slot = timer.current_slot();
                        let parent = cryptarchia.tip();
                        let note_tree = cryptarchia.tip_state().lead_commitments();
                        tracing::debug!("ticking for slot {}", u64::from(slot));

                        let Some(epoch_state) = cryptarchia.epoch_state_for_slot(slot) else {
                            tracing::error!("trying to propose a block for slot {} but epoch state is not available", u64::from(slot));
                            continue;
                        };
                        if let Some(proof) = leader.build_proof_for(note_tree, epoch_state, slot, parent).await {
                            tracing::debug!("proposing block...");
                            // TODO: spawn as a separate task?
                            let block = Self::propose_block(
                                parent,
                                slot,
                                proof,
                                tx_selector.clone(),
                                blob_selector.clone(),
                                &relays
                            ).await;

                            if let Some(block) = block {
                                blend_adapter.blend(block).await;
                            }
                        }
                    }

                    Some(msg) = self.service_state.inbound_relay.next() => {
                        Self::process_message(&cryptarchia, &self.block_subscription_sender, msg);
                    }
                    Some(msg) = lifecycle_stream.next() => {
                        if Self::should_stop_service(msg).await {
                            break;
                        }
                    }
                }
            }
            // it sucks to use "Cryptarchia" when we have the Self::SERVICE_ID.
            // Somehow it just do not let refer to the type to reference it.
            // Probably related to too many generics.
        }.instrument(span!(Level::TRACE, CRYPTARCHIA_ID)).await;
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CryptarchiaConsensusState<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings> {
    tip: Option<HeaderId>,
    security_block: Option<HeaderId>,
    _txs: PhantomData<TxS>,
    _bxs: PhantomData<BxS>,
    _network_adapter_settings: PhantomData<NetworkAdapterSettings>,
    _blend_adapter_settings: PhantomData<BlendAdapterSettings>,
}

impl<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>
    CryptarchiaConsensusState<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>
{
    pub fn new(tip: Option<HeaderId>, security_block: Option<HeaderId>) -> Self {
        Self {
            tip,
            security_block,
            _txs: Default::default(),
            _bxs: Default::default(),
            _network_adapter_settings: Default::default(),
            _blend_adapter_settings: Default::default(),
        }
    }

    pub fn should_recover(&self) -> bool {
        self.tip.is_some()
    }

    pub fn can_recover(&self) -> bool {
        self.should_recover() && self.security_block.is_some()
    }
}

impl<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings> ServiceState
    for CryptarchiaConsensusState<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>
{
    type Settings = CryptarchiaSettings<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>;
    type Error = Error;

    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self::new(None, None))
    }
}

impl<
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
    >
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
    >
where
    NetAdapter: NetworkAdapter + Clone + Send + Sync + 'static,
    NetAdapter::Settings: Send,
    BlendAdapter: blend::BlendAdapter + Clone + Send + Sync + 'static,
    BlendAdapter::Settings: Send,
    ClPool: MemPool<BlockId = HeaderId> + Send + Sync + 'static,
    ClPool::Settings: Send + Sync + 'static,
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
    ClPool::Key: Debug + Send + Sync,
    ClPoolAdapter:
        MempoolAdapter<Payload = ClPool::Item, Key = ClPool::Key> + Send + Sync + 'static,
    DaPool::Item: DispersedBlobInfo<BlobId = DaPool::Key>
        + BlobMetadata
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    DaPool: MemPool<BlockId = HeaderId, Key = SamplingBackend::BlobId> + Send + Sync + 'static,
    DaPool::Settings: Send + Sync + 'static,
    DaPoolAdapter: MempoolAdapter<Key = DaPool::Key> + Send + Sync + 'static,
    DaPoolAdapter::Payload: DispersedBlobInfo + Into<DaPool::Item> + Debug,
    TxS: TxSelect<Tx = ClPool::Item> + Clone + Send + Sync + 'static,
    TxS::Settings: Send,
    BS: BlobSelect<BlobId = DaPool::Item> + Clone + Send + Sync + 'static,
    BS::Settings: Send,
    Storage: StorageBackend + Send + Sync + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::BlobId: Debug + Ord + Send + Sync + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingRng: SeedableRng + RngCore,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
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

    fn process_message(
        cryptarchia: &Cryptarchia,
        block_channel: &broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
        msg: ConsensusMsg<Block<ClPool::Item, DaPool::Item>>,
    ) {
        match msg {
            ConsensusMsg::Info { tx } => {
                let info = CryptarchiaInfo {
                    tip: cryptarchia.tip(),
                    slot: cryptarchia
                        .ledger
                        .state(&cryptarchia.tip())
                        .expect("tip state not available")
                        .slot(),
                    height: cryptarchia
                        .consensus
                        .branches()
                        .get(&cryptarchia.tip())
                        .expect("tip branch not available")
                        .length(),
                };
                tx.send(info).unwrap_or_else(|e| {
                    tracing::error!("Could not send consensus info through channel: {:?}", e)
                });
            }
            ConsensusMsg::BlockSubscribe { sender } => {
                sender.send(block_channel.subscribe()).unwrap_or_else(|_| {
                    tracing::error!("Could not subscribe to block subscription channel")
                });
            }
            ConsensusMsg::GetHeaders { from, to, tx } => {
                // default to tip block if not present
                let from = from.unwrap_or(cryptarchia.tip());
                // default to genesis block if not present
                let to = to.unwrap_or(cryptarchia.genesis());

                let mut res = Vec::new();
                let mut cur = from;

                let branches = cryptarchia.consensus.branches();
                while let Some(h) = branches.get(&cur) {
                    res.push(h.id());
                    // limit the response size
                    if cur == to || cur == cryptarchia.genesis() || res.len() >= HEADERS_LIMIT {
                        break;
                    }
                    cur = h.parent();
                }

                tx.send(res)
                    .unwrap_or_else(|_| tracing::error!("could not send blocks through channel"));
            }
        }
    }

    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    #[instrument(level = "debug", skip(cryptarchia, leader, relays))]
    async fn process_block(
        mut cryptarchia: Cryptarchia,
        leader: &mut leadership::Leader,
        block: Block<ClPool::Item, DaPool::Item>,
        relays: &CryptarchiaConsensusRelays<
            BlendAdapter,
            BS,
            ClPool,
            ClPoolAdapter,
            DaPool,
            DaPoolAdapter,
            NetAdapter,
            SamplingBackend,
            SamplingRng,
            Storage,
            TxS,
        >,
        block_broadcaster: &mut broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
    ) -> Cryptarchia {
        tracing::debug!("received proposal {:?}", block);

        // TODO: filter on time?

        let header = block.header();
        let id = header.id();
        let sampled_blobs = match get_sampled_blobs(relays.sampling_relay().clone()).await {
            Ok(sampled_blobs) => sampled_blobs,
            Err(error) => {
                error!("Unable to retrieved sampled blobs: {error}");
                return cryptarchia;
            }
        };
        if !Self::validate_block(&block, &sampled_blobs) {
            error!("Invalid block: {block:?}");
            return cryptarchia;
        }

        match cryptarchia.try_apply_header(header) {
            Ok(new_state) => {
                // update leader
                leader.follow_chain(header.parent(), id, header.leader_proof().nullifier());

                // remove included content from mempool
                mark_in_block(
                    relays.cl_mempool_relay().clone(),
                    block.transactions().map(Transaction::hash),
                    id,
                )
                .await;
                mark_in_block(
                    relays.da_mempool_relay().clone(),
                    block.blobs().map(DispersedBlobInfo::blob_id),
                    id,
                )
                .await;

                mark_blob_in_block(
                    relays.sampling_relay().clone(),
                    block.blobs().map(DispersedBlobInfo::blob_id).collect(),
                )
                .await;

                // store block
                let msg = <StorageMsg<_>>::new_store_message(header.id(), block.clone());
                if let Err((e, _msg)) = relays.storage_adapter().storage_relay.send(msg).await {
                    tracing::error!("Could not send block to storage: {e}");
                }

                if let Err(e) = block_broadcaster.send(block) {
                    tracing::error!("Could not notify block to services {e}");
                }

                cryptarchia = new_state;
            }
            Err(Error::Ledger(nomos_ledger::LedgerError::ParentNotFound(parent)))
            | Err(Error::Consensus(cryptarchia_engine::Error::ParentMissing(parent))) => {
                tracing::debug!("missing parent {:?}", parent);
                // TODO: request parent block
            }
            Err(e) => tracing::debug!("invalid block {:?}: {e:?}", block),
        }

        cryptarchia
    }

    #[allow(clippy::too_many_arguments)]
    #[instrument(level = "debug", skip(tx_selector, blob_selector, relays))]
    async fn propose_block(
        parent: HeaderId,
        slot: Slot,
        proof: Risc0LeaderProof,
        tx_selector: TxS,
        blob_selector: BS,
        relays: &CryptarchiaConsensusRelays<
            BlendAdapter,
            BS,
            ClPool,
            ClPoolAdapter,
            DaPool,
            DaPoolAdapter,
            NetAdapter,
            SamplingBackend,
            SamplingRng,
            Storage,
            TxS,
        >,
    ) -> Option<Block<ClPool::Item, DaPool::Item>> {
        let mut output = None;
        let cl_txs = get_mempool_contents(relays.cl_mempool_relay().clone());
        let da_certs = get_mempool_contents(relays.da_mempool_relay().clone());
        let blobs_ids = get_sampled_blobs(relays.sampling_relay().clone());
        match futures::join!(cl_txs, da_certs, blobs_ids) {
            (Ok(cl_txs), Ok(da_blobs_info), Ok(blobs_ids)) => {
                let block = BlockBuilder::new(
                    tx_selector,
                    blob_selector,
                    Builder::new(parent, slot, proof),
                )
                .with_transactions(cl_txs)
                .with_blobs_info(
                    da_blobs_info.filter(move |info| blobs_ids.contains(&info.blob_id())),
                )
                .build()
                .expect("Proposal block should always succeed to be built");
                tracing::debug!("proposed block with id {:?}", block.header().id());
                output = Some(block);
            }
            (tx_error, da_certificate_error, blobs_error) => {
                if let Err(_tx_error) = tx_error {
                    tracing::error!("Could not fetch block cl transactions");
                }
                if let Err(_da_certificate_error) = da_certificate_error {
                    tracing::error!("Could not fetch block da certificates");
                }
                if let Err(_blobs_error) = blobs_error {
                    tracing::error!("Could not fetch block da blobs");
                }
            }
        }
        output
    }

    fn validate_block(
        block: &Block<ClPool::Item, DaPool::Item>,
        sampled_blobs_ids: &BTreeSet<DaPool::Key>,
    ) -> bool {
        let validated_blobs = block
            .blobs()
            .all(|blob| sampled_blobs_ids.contains(&blob.blob_id()));
        validated_blobs
    }

    fn log_received_block(block: &Block<ClPool::Item, DaPool::Item>) {
        let content_size = block.header().content_size();
        let transactions = block.cl_transactions_len();
        let blobs = block.bl_blobs_len();

        tracing::info!(
            counter.received_blocks = 1,
            transactions = transactions,
            blobs = blobs,
            bytes = content_size
        );
        tracing::info!(
            histogram.received_blocks_data = content_size,
            transactions = transactions,
            blobs = blobs
        );
    }

    async fn get_blocks_from_tip(
        tip: HeaderId,
        security_block_header: HeaderId,
        storage_adapter: &StorageAdapter<Storage, TxS::Tx, BS::BlobId>,
    ) -> Vec<Block<ClPool::Item, DaPool::Item>> {
        let blocks_from_tip = futures::stream::unfold(tip, |header_id| async move {
            if header_id == security_block_header {
                None
            } else {
                let block = storage_adapter
                    .get_block(&header_id)
                    .await
                    .unwrap_or_else(|| {
                        panic!("Could not retrieve block {tip} from storage during recovery")
                    });
                let parent_header_id = block.header().parent();
                Some((block, parent_header_id))
            }
        });

        blocks_from_tip.collect::<Vec<_>>().await
    }

    /// Restore the cryptarchia state from the
    async fn restore_from_recovery(
        mut cryptarchia: Cryptarchia,
        tip: HeaderId,
        security_block_header: HeaderId,
        leader: &mut leadership::Leader,
        relays: &CryptarchiaConsensusRelays<
            BlendAdapter,
            BS,
            ClPool,
            ClPoolAdapter,
            DaPool,
            DaPoolAdapter,
            A,
            SamplingBackend,
            SamplingRng,
            Storage,
            TxS,
        >,
        block_broadcaster: &mut broadcast::Sender<Block<ClPool::Item, DaPool::Item>>,
    ) -> Cryptarchia {
        let blocks_to_tip =
            Self::get_blocks_from_tip(tip, security_block_header, relays.storage_adapter())
                .await
                .into_iter()
                .rev();

        for block in blocks_to_tip {
            cryptarchia =
                Self::process_block(cryptarchia, leader, block, relays, block_broadcaster).await
        }

        cryptarchia
    }
}

#[derive(Debug)]
pub enum ConsensusMsg<Block> {
    Info {
        tx: Sender<CryptarchiaInfo>,
    },
    BlockSubscribe {
        sender: oneshot::Sender<broadcast::Receiver<Block>>,
    },
    GetHeaders {
        from: Option<HeaderId>,
        to: Option<HeaderId>,
        tx: Sender<Vec<HeaderId>>,
    },
}

impl<Block: 'static> RelayMessage for ConsensusMsg<Block> {}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CryptarchiaInfo {
    pub tip: HeaderId,
    pub slot: Slot,
    pub height: u64,
}

async fn get_mempool_contents<Payload, Item, Key>(
    mempool: OutboundRelay<MempoolMsg<HeaderId, Payload, Item, Key>>,
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

async fn mark_in_block<Payload, Item, Key>(
    mempool: OutboundRelay<MempoolMsg<HeaderId, Payload, Item, Key>>,
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

async fn mark_blob_in_block<BlobId: Debug>(
    sampling_relay: SamplingRelay<BlobId>,
    blobs_id: Vec<BlobId>,
) {
    if let Err((_e, DaSamplingServiceMsg::MarkInBlock { blobs_id })) = sampling_relay
        .send(DaSamplingServiceMsg::MarkInBlock { blobs_id })
        .await
    {
        error!("Error marking in block for blobs ids: {blobs_id:?}");
    }
}

async fn get_sampled_blobs<BlobId>(
    sampling_relay: SamplingRelay<BlobId>,
) -> Result<BTreeSet<BlobId>, DynError> {
    let (sender, receiver) = oneshot::channel();
    sampling_relay
        .send(DaSamplingServiceMsg::GetValidatedBlobs {
            reply_channel: sender,
        })
        .await
        .map_err(|(error, _)| Box::new(error) as DynError)?;
    receiver.await.map_err(|error| Box::new(error) as DynError)
}

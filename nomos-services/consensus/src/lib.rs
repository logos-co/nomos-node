pub mod committee_membership;
pub mod leader_selection;
pub mod network;
mod tally;
mod task_manager;

// std
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::pin::Pin;
use std::time::Duration;
// crates
use bls_signatures::PrivateKey;
pub use consensus_engine::NodeId;
use futures::{Stream, StreamExt};
use leader_selection::UpdateableLeaderSelection;
use serde::Deserialize;
use serde::{de::DeserializeOwned, Serialize};
use serde_with::serde_as;
use tokio::sync::oneshot::Sender;
use tracing::instrument;
// internal
use crate::network::messages::{
    NetworkMessage, NewViewMsg, ProposalMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg,
};
use crate::network::NetworkAdapter;
use crate::tally::{
    happy::CarnotTally, timeout::TimeoutTally, unhappy::NewViewTally, CarnotTallySettings,
};
use consensus_engine::{
    overlay::RandomBeaconState, AggregateQc, BlockId, Carnot, Committee, LeaderProof, NewView,
    Overlay, Payload, Qc, StandardQc, Timeout, TimeoutQc, View, Vote,
};
use task_manager::TaskManager;

use crate::committee_membership::UpdateableCommitteeMembership;
use nomos_core::block::builder::BlockBuilder;
use nomos_core::block::Block;
use nomos_core::da::certificate::{BlobCertificateSelect, Certificate};
use nomos_core::tx::{Transaction, TxSelect};
use nomos_core::vote::Tally;
use nomos_mempool::{
    backend::MemPool, network::NetworkAdapter as MempoolAdapter, Certificate as CertDiscriminant,
    MempoolMsg, MempoolService, Transaction as TxDiscriminant,
};
use nomos_network::NetworkService;
use nomos_storage::{backends::StorageBackend, StorageMsg, StorageService};
use overwatch_rs::services::relay::{OutboundRelay, Relay, RelayMessage};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

fn default_timeout() -> Duration {
    DEFAULT_TIMEOUT
}

// Random seed for each round provided by the protocol
pub type Seed = [u8; 32];

#[derive(Debug, Deserialize, Serialize)]
pub struct CarnotSettings<O: Overlay, Ts, Bs> {
    pub private_key: [u8; 32],
    pub overlay_settings: O::Settings,
    #[serde(default = "default_timeout")]
    pub timeout: Duration,
    #[serde(default)]
    pub transaction_selector_settings: Ts,
    #[serde(default)]
    pub blob_selector_settings: Bs,
}

impl<O: Overlay, Ts: Clone, Bs: Clone> Clone for CarnotSettings<O, Ts, Bs> {
    fn clone(&self) -> Self {
        Self {
            private_key: self.private_key,
            overlay_settings: self.overlay_settings.clone(),
            timeout: self.timeout,
            transaction_selector_settings: self.transaction_selector_settings.clone(),
            blob_selector_settings: self.blob_selector_settings.clone(),
        }
    }
}

impl<O: Overlay, Ts, Bs> CarnotSettings<O, Ts, Bs> {
    #[inline]
    pub const fn new(
        private_key: [u8; 32],
        overlay_settings: O::Settings,
        transaction_selector_settings: Ts,
        blob_selector_settings: Bs,
        timeout: Duration,
    ) -> Self {
        Self {
            private_key,
            overlay_settings,
            timeout,
            transaction_selector_settings,
            blob_selector_settings,
        }
    }
}

pub struct CarnotConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage>
where
    A: NetworkAdapter,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key>,
    ClPool: MemPool,
    DaPool: MemPool,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key>,
    O: Overlay + Debug,
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
    // when implementing ServiceCore for CarnotConsensus
    network_relay: Relay<NetworkService<A::Backend>>,
    cl_mempool_relay: Relay<MempoolService<ClPoolAdapter, ClPool, TxDiscriminant>>,
    da_mempool_relay: Relay<MempoolService<DaPoolAdapter, DaPool, CertDiscriminant>>,
    storage_relay: Relay<StorageService<Storage>>,
    _overlay: std::marker::PhantomData<O>,
}

impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage> ServiceData
    for CarnotConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage>
where
    A: NetworkAdapter,
    ClPool: MemPool,
    ClPool::Item: Debug,
    ClPool::Key: Debug,
    DaPool: MemPool,
    DaPool::Item: Debug,
    DaPool::Key: Debug,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key>,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key>,
    O: Overlay + Debug,
    TxS: TxSelect<Tx = ClPool::Item>,
    BS: BlobCertificateSelect<Certificate = DaPool::Item>,
    Storage: StorageBackend + Send + Sync + 'static,
{
    const SERVICE_ID: ServiceId = "Carnot";
    type Settings = CarnotSettings<O, TxS::Settings, BS::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = ConsensusMsg;
}

#[async_trait::async_trait]
impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage> ServiceCore
    for CarnotConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage>
where
    A: NetworkAdapter + Clone + Send + Sync + 'static,
    ClPool: MemPool + Send + Sync + 'static,
    ClPool::Settings: Send + Sync + 'static,
    DaPool: MemPool + Send + Sync + 'static,
    DaPool::Settings: Send + Sync + 'static,
    ClPool::Item: Transaction
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
    DaPool::Item: Certificate
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
    O: Overlay + Debug + Send + Sync + 'static,
    O::LeaderSelection: UpdateableLeaderSelection,
    O::CommitteeMembership: UpdateableCommitteeMembership,
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
            _overlay: Default::default(),
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

        let CarnotSettings {
            private_key,
            overlay_settings,
            timeout,
            transaction_selector_settings,
            blob_selector_settings,
        } = self.service_state.settings_reader.get_updated_settings();

        let overlay = O::new(overlay_settings);
        let genesis = consensus_engine::Block {
            id: BlockId::zeros(),
            view: View::new(0),
            parent_qc: Qc::Standard(StandardQc::genesis()),
            leader_proof: LeaderProof::LeaderId {
                leader_id: NodeId::new([0; 32]),
            },
        };
        let mut carnot = Carnot::from_genesis(NodeId::new(private_key), genesis, overlay);
        let adapter = A::new(network_relay).await;
        let private_key = PrivateKey::new(private_key);
        let self_committee = carnot.self_committee();
        let leader_committee = [carnot.id()].into_iter().collect::<Committee>();
        let tally_settings = CarnotTallySettings {
            threshold: carnot.super_majority_threshold(),
            participating_nodes: carnot.child_committees().into_iter().flatten().collect(),
        };
        let leader_tally_settings = CarnotTallySettings {
            threshold: carnot.leader_super_majority_threshold(),
            participating_nodes: carnot.root_committee(),
        };

        let tx_selector = TxS::new(transaction_selector_settings);
        let blob_selector = BS::new(blob_selector_settings);

        let mut task_manager = TaskManager::new();

        let genesis_block = carnot.genesis_block();
        Self::process_view_change(
            carnot.clone(),
            genesis_block.view.prev(),
            &mut task_manager,
            adapter.clone(),
            timeout,
        )
        .await;
        // we already have the genesis block, no need to wait for it
        task_manager.push(
            genesis_block.view,
            Self::gather_votes(
                adapter.clone(),
                self_committee.clone(),
                genesis_block.clone(),
                tally_settings.clone(),
            ),
        );

        if carnot.is_next_leader() {
            let network_adapter = adapter.clone();
            task_manager.push(genesis_block.view.next(), async move {
                let Event::Approve { qc, .. } = Self::gather_votes(
                    network_adapter,
                    leader_committee.clone(),
                    genesis_block,
                    leader_tally_settings.clone(),
                )
                .await
                else {
                    tracing::debug!("Failed to gather initial votes");
                    return Event::None;
                };
                Event::ProposeBlock { qc }
            });
        }

        loop {
            tokio::select! {
                    Some(event) = task_manager.next() => {
                        carnot = Self::process_carnot_event(
                            carnot,
                            event,
                            &mut task_manager,
                            adapter.clone(),
                            private_key,
                            cl_mempool_relay.clone(),
                            da_mempool_relay.clone(),
                            storage_relay.clone(),
                            tx_selector.clone(),
                            blob_selector.clone(),
                            timeout,
                        )
                        .await
                    }
                    Some(msg) = self.service_state.inbound_relay.next() => {
                        Self::process_message(&carnot, msg);
                    }
            }
        }
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Output<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> {
    Send(consensus_engine::Send),
    BroadcastTimeoutQc {
        timeout_qc: TimeoutQc,
    },
    BroadcastProposal {
        proposal: Block<Tx, BlobCertificate>,
    },
}

impl<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage>
    CarnotConsensus<A, ClPool, ClPoolAdapter, DaPool, DaPoolAdapter, O, TxS, BS, Storage>
where
    A: NetworkAdapter + Clone + Send + Sync + 'static,
    ClPool: MemPool + Send + Sync + 'static,
    ClPool::Settings: Send + Sync + 'static,
    DaPool: MemPool + Send + Sync + 'static,
    DaPool::Settings: Send + Sync + 'static,
    ClPool::Item: Transaction
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
    DaPool::Item: Certificate
        + Debug
        + Clone
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    O: Overlay + Debug + Send + Sync + 'static,
    O::LeaderSelection: UpdateableLeaderSelection,
    O::CommitteeMembership: UpdateableCommitteeMembership,
    TxS: TxSelect<Tx = ClPool::Item> + Clone + Send + Sync + 'static,
    BS: BlobCertificateSelect<Certificate = DaPool::Item> + Clone + Send + Sync + 'static,
    ClPool::Key: Debug + Send + Sync,
    DaPool::Key: Debug + Send + Sync,
    ClPoolAdapter: MempoolAdapter<Item = ClPool::Item, Key = ClPool::Key> + Send + Sync + 'static,
    DaPoolAdapter: MempoolAdapter<Item = DaPool::Item, Key = DaPool::Key> + Send + Sync + 'static,
    Storage: StorageBackend + Send + Sync + 'static,
{
    fn process_message(carnot: &Carnot<O>, msg: ConsensusMsg) {
        match msg {
            ConsensusMsg::Info { tx } => {
                let info = CarnotInfo {
                    id: carnot.id(),
                    current_view: carnot.current_view(),
                    highest_voted_view: carnot.highest_voted_view(),
                    local_high_qc: carnot.high_qc(),
                    safe_blocks: carnot.safe_blocks().clone(),
                    last_view_timeout_qc: carnot.last_view_timeout_qc(),
                    committed_blocks: carnot.latest_committed_blocks(),
                };
                tx.send(info).unwrap_or_else(|e| {
                    tracing::error!("Could not send consensus info through channel: {:?}", e)
                });
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_carnot_event(
        mut carnot: Carnot<O>,
        event: Event<ClPool::Item, DaPool::Item>,
        task_manager: &mut TaskManager<View, Event<ClPool::Item, DaPool::Item>>,
        adapter: A,
        private_key: PrivateKey,
        cl_mempool_relay: OutboundRelay<MempoolMsg<ClPool::Item, ClPool::Key>>,
        da_mempool_relay: OutboundRelay<MempoolMsg<DaPool::Item, DaPool::Key>>,
        storage_relay: OutboundRelay<StorageMsg<Storage>>,
        tx_selector: TxS,
        blobl_selector: BS,
        timeout: Duration,
    ) -> Carnot<O> {
        let mut output = None;
        let prev_view = carnot.current_view();
        match event {
            Event::Proposal { block, stream } => {
                (carnot, output) = Self::process_block(
                    carnot,
                    block,
                    stream,
                    task_manager,
                    adapter.clone(),
                    storage_relay,
                )
                .await;
            }
            Event::Approve { block, .. } => {
                tracing::debug!("approving proposal {:?}", block);
                let (new_carnot, out) = carnot.approve_block(block);
                carnot = new_carnot;
                output = Some(Output::Send::<ClPool::Item, DaPool::Item>(out));
            }
            Event::LocalTimeout { view } => {
                tracing::debug!("local timeout");
                let (new_carnot, out) = carnot.local_timeout();
                carnot = new_carnot;
                output = out.map(Output::Send);
                // keep timeout until the situation is resolved
                task_manager.push(view, async move {
                    tokio::time::sleep(timeout).await;
                    Event::LocalTimeout { view }
                });
            }
            Event::NewView {
                timeout_qc,
                new_views,
            } => {
                (carnot, output) = Self::approve_new_view(
                    carnot,
                    timeout_qc,
                    new_views,
                    task_manager,
                    adapter.clone(),
                )
                .await;
            }
            Event::TimeoutQc { timeout_qc } => {
                (carnot, output) =
                    Self::receive_timeout_qc(carnot, timeout_qc, task_manager, adapter.clone())
                        .await;
            }
            Event::RootTimeout { timeouts } => {
                (carnot, output) = Self::process_root_timeout(carnot, timeouts).await;
            }
            Event::ProposeBlock { qc } => {
                output = Self::propose_block(
                    carnot.id(),
                    private_key,
                    qc,
                    tx_selector.clone(),
                    blobl_selector.clone(),
                    cl_mempool_relay,
                    da_mempool_relay,
                )
                .await;
            }
            _ => {}
        }

        let current_view = carnot.current_view();
        if current_view != prev_view {
            Self::process_view_change(
                carnot.clone(),
                prev_view,
                task_manager,
                adapter.clone(),
                timeout,
            )
            .await;
        }

        if let Some(output) = output {
            handle_output(&adapter, carnot.id(), output).await;
        }

        carnot
    }

    #[allow(clippy::type_complexity)]
    #[instrument(level = "debug", skip(adapter, task_manager, stream, storage_relay))]
    async fn process_block(
        mut carnot: Carnot<O>,
        block: Block<ClPool::Item, DaPool::Item>,
        mut stream: Pin<Box<dyn Stream<Item = Block<ClPool::Item, DaPool::Item>> + Send>>,
        task_manager: &mut TaskManager<View, Event<ClPool::Item, DaPool::Item>>,
        adapter: A,
        storage_relay: OutboundRelay<StorageMsg<Storage>>,
    ) -> (Carnot<O>, Option<Output<ClPool::Item, DaPool::Item>>) {
        tracing::debug!("received proposal {:?}", block);
        if carnot.highest_voted_view() >= block.header().view {
            tracing::debug!("already voted for view {}", block.header().view);
            return (carnot, None);
        }

        let original_block = block;
        let block = original_block.header().clone();

        let self_committee = carnot.self_committee();
        let leader_committee = [carnot.id()].into_iter().collect();

        let tally_settings = CarnotTallySettings {
            threshold: carnot.super_majority_threshold(),
            participating_nodes: carnot.child_committees().into_iter().flatten().collect(),
        };
        let leader_tally_settings = CarnotTallySettings {
            threshold: carnot.leader_super_majority_threshold(),
            // TODO: add children of root committee
            participating_nodes: carnot.root_committee(),
        };

        match carnot.receive_block(block.clone()) {
            Ok(mut new_state) => {
                let new_view = new_state.current_view();
                let msg = <StorageMsg<_>>::new_store_message(block.id, original_block.clone());
                if let Err((e, _msg)) = storage_relay.send(msg).await {
                    tracing::error!("Could not send block to storage: {e}");
                }
                if new_view != carnot.current_view() {
                    task_manager.push(
                        block.view,
                        Self::gather_votes(
                            adapter.clone(),
                            self_committee,
                            block.clone(),
                            tally_settings,
                        ),
                    );
                    new_state = Self::update_overlay(
                        new_state,
                        |leader_selection| leader_selection.on_new_block_received(&original_block),
                        |committee_membership| {
                            committee_membership.on_new_block_received(&original_block)
                        },
                    );
                } else {
                    task_manager.push(block.view, async move {
                        if let Some(block) = stream.next().await {
                            Event::Proposal { block, stream }
                        } else {
                            Event::None
                        }
                    });
                }
                carnot = new_state;
            }
            Err(_) => tracing::debug!("invalid block {:?}", block),
        }

        if carnot.is_next_leader() {
            task_manager.push(block.view, async move {
                let Event::Approve { qc, .. } =
                    Self::gather_votes(adapter, leader_committee, block, leader_tally_settings)
                        .await
                else {
                    unreachable!()
                };
                Event::ProposeBlock { qc }
            });
        }

        (carnot, None)
    }

    #[allow(clippy::type_complexity)]
    #[instrument(level = "debug", skip(task_manager, adapter))]
    async fn approve_new_view(
        carnot: Carnot<O>,
        timeout_qc: TimeoutQc,
        new_views: HashSet<NewView>,
        task_manager: &mut TaskManager<View, Event<ClPool::Item, DaPool::Item>>,
        adapter: A,
    ) -> (Carnot<O>, Option<Output<ClPool::Item, DaPool::Item>>) {
        let leader_committee = [carnot.id()].into_iter().collect();
        let leader_tally_settings = CarnotTallySettings {
            threshold: carnot.leader_super_majority_threshold(),
            // TODO: add children of root committee
            participating_nodes: carnot.root_committee(),
        };
        let (new_carnot, out) = carnot.approve_new_view(timeout_qc.clone(), new_views);
        let new_view = timeout_qc.view().next();
        if carnot.is_next_leader() {
            let high_qc = carnot.high_qc();
            task_manager.push(new_view, async move {
                let _votes = Self::gather_new_views(
                    adapter,
                    leader_committee,
                    timeout_qc,
                    leader_tally_settings.clone(),
                )
                .await;
                Event::ProposeBlock {
                    qc: Qc::Aggregated(AggregateQc {
                        high_qc,
                        view: new_view,
                    }),
                }
            });
        }

        (new_carnot, Some(Output::Send(out)))
    }

    #[allow(clippy::type_complexity)]
    #[instrument(level = "debug", skip(task_manager, adapter))]
    async fn receive_timeout_qc(
        carnot: Carnot<O>,
        timeout_qc: TimeoutQc,
        task_manager: &mut TaskManager<View, Event<ClPool::Item, DaPool::Item>>,
        adapter: A,
    ) -> (Carnot<O>, Option<Output<ClPool::Item, DaPool::Item>>) {
        let mut new_state = carnot.receive_timeout_qc(timeout_qc.clone());
        let self_committee = carnot.self_committee();
        let tally_settings = CarnotTallySettings {
            threshold: carnot.super_majority_threshold(),
            participating_nodes: carnot.child_committees().into_iter().flatten().collect(),
        };
        task_manager.push(
            timeout_qc.view().next(),
            Self::gather_new_views(adapter, self_committee, timeout_qc.clone(), tally_settings),
        );
        if carnot.current_view() != new_state.current_view() {
            new_state = Self::update_overlay(
                new_state,
                |leader_selection| leader_selection.on_timeout_qc_received(&timeout_qc),
                |committee_membership| committee_membership.on_timeout_qc_received(&timeout_qc),
            );
        }
        (new_state, None)
    }

    #[allow(clippy::type_complexity)]
    #[instrument(level = "debug")]
    async fn process_root_timeout(
        carnot: Carnot<O>,
        timeouts: HashSet<Timeout>,
    ) -> (Carnot<O>, Option<Output<ClPool::Item, DaPool::Item>>) {
        // we might have received a timeout_qc sent by some other node and advanced the view
        // already, in which case we should ignore the timeout
        if carnot.current_view()
            != timeouts
                .iter()
                .map(|t| t.view)
                .max()
                .unwrap_or(View::new(0))
        {
            return (carnot, None);
        }

        assert!(
            timeouts.iter().all(|t| t.view == carnot.current_view()),
            "{:?} {}",
            timeouts.iter().collect::<Vec<_>>(),
            carnot.current_view(),
        );
        let high_qc = timeouts
            .iter()
            .map(|t| &t.high_qc)
            .chain(std::iter::once(&carnot.high_qc()))
            .max_by_key(|qc| qc.view)
            .expect("empty root committee")
            .clone();
        let mut output = None;
        if carnot.is_member_of_root_committee() {
            let timeout_qc = TimeoutQc::new(carnot.current_view(), high_qc, carnot.id());
            output = Some(Output::BroadcastTimeoutQc { timeout_qc });
        }
        (carnot, output)
    }

    #[instrument(
        level = "debug",
        skip(
            cl_mempool_relay,
            da_mempool_relay,
            private_key,
            tx_selector,
            blob_selector
        )
    )]
    async fn propose_block(
        id: NodeId,
        private_key: PrivateKey,
        qc: Qc,
        tx_selector: TxS,
        blob_selector: BS,
        cl_mempool_relay: OutboundRelay<MempoolMsg<ClPool::Item, ClPool::Key>>,
        da_mempool_relay: OutboundRelay<MempoolMsg<DaPool::Item, DaPool::Key>>,
    ) -> Option<Output<ClPool::Item, DaPool::Item>> {
        let mut output = None;
        let cl_txs = get_mempool_contents(cl_mempool_relay);
        let da_certs = get_mempool_contents(da_mempool_relay);

        match futures::join!(cl_txs, da_certs) {
            (Ok(cl_txs), Ok(da_certs)) => {
                let beacon = RandomBeaconState::generate_happy(qc.view(), &private_key);
                let Ok(proposal) = BlockBuilder::new(tx_selector, blob_selector)
                    .with_view(qc.view().next())
                    .with_parent_qc(qc)
                    .with_proposer(id)
                    .with_beacon_state(beacon)
                    .with_transactions(cl_txs)
                    .with_blobs_certificates(da_certs)
                    .build()
                else {
                    panic!("Proposal block should always succeed to be built")
                };
                output = Some(Output::BroadcastProposal { proposal });
            }
            (Err(_), _) => tracing::error!("Could not fetch block cl transactions"),
            (_, Err(_)) => tracing::error!("Could not fetch block da certificates"),
        }
        output
    }

    async fn process_view_change(
        carnot: Carnot<O>,
        prev_view: View,
        task_manager: &mut TaskManager<View, Event<ClPool::Item, DaPool::Item>>,
        adapter: A,
        timeout: Duration,
    ) {
        let current_view = carnot.current_view();
        // First we cancel previous processing view tasks
        task_manager.cancel(prev_view);
        tracing::debug!("Advanced view from {prev_view} to {current_view}");
        // View change!
        task_manager.push(current_view, async move {
            tokio::time::sleep(timeout).await;
            Event::LocalTimeout { view: current_view }
        });
        task_manager.push(
            current_view.next(),
            Self::gather_block(adapter.clone(), current_view.next()),
        );
        task_manager.push(
            current_view,
            Self::gather_timeout_qc(adapter.clone(), current_view),
        );
        if carnot.is_member_of_root_committee() {
            task_manager.push(
                current_view,
                Self::gather_timeout(
                    adapter,
                    carnot.self_committee(),
                    current_view,
                    CarnotTallySettings {
                        threshold: carnot.leader_super_majority_threshold(),
                        participating_nodes: carnot.root_committee(),
                    },
                ),
            );
        }
    }

    async fn gather_timeout_qc(
        adapter: A,
        view: consensus_engine::View,
    ) -> Event<ClPool::Item, DaPool::Item> {
        if let Some(timeout_qc) = adapter
            .timeout_qc_stream(view)
            .await
            .map(|msg| msg.qc)
            .next()
            .await
        {
            Event::TimeoutQc { timeout_qc }
        } else {
            Event::None
        }
    }

    #[instrument(level = "debug", skip(adapter, tally))]
    async fn gather_votes(
        adapter: A,
        committee: Committee,
        block: consensus_engine::Block,
        tally: CarnotTallySettings,
    ) -> Event<ClPool::Item, DaPool::Item> {
        let tally = CarnotTally::new(tally);
        let votes_stream = adapter.votes_stream(&committee, block.view, block.id).await;
        match tally.tally(block.clone(), votes_stream).await {
            Ok((qc, votes)) => Event::Approve { qc, votes, block },
            Err(e) => {
                tracing::debug!("Error gathering votes: {e}");
                Event::None
            }
        }
    }

    #[instrument(level = "debug", skip(adapter, tally))]
    async fn gather_new_views(
        adapter: A,
        committee: Committee,
        timeout_qc: TimeoutQc,
        tally: CarnotTallySettings,
    ) -> Event<ClPool::Item, DaPool::Item> {
        let tally = NewViewTally::new(tally);
        let stream = adapter
            .new_view_stream(&committee, timeout_qc.view().next())
            .await;
        match tally.tally(timeout_qc.clone(), stream).await {
            Ok((_qc, new_views)) => Event::NewView {
                timeout_qc,
                new_views,
            },
            Err(_e) => {
                todo!("Handle tally error {_e}");
            }
        }
    }

    #[instrument(level = "debug", skip(adapter, tally))]
    async fn gather_timeout(
        adapter: A,
        committee: Committee,
        view: consensus_engine::View,
        tally: CarnotTallySettings,
    ) -> Event<ClPool::Item, DaPool::Item> {
        let tally = TimeoutTally::new(tally);
        let stream = adapter.timeout_stream(&committee, view).await;
        match tally.tally(view, stream).await {
            Ok((_, timeouts)) => Event::RootTimeout { timeouts },
            Err(_e) => {
                todo!("Handle tally error {_e}");
            }
        }
    }

    #[instrument(level = "debug", skip(adapter))]
    async fn gather_block(
        adapter: A,
        view: consensus_engine::View,
    ) -> Event<ClPool::Item, DaPool::Item> {
        let stream = adapter
            .proposal_chunks_stream(view)
            .await
            .filter_map(move |msg| {
                async move {
                    let proposal = Block::from_bytes(&msg.data);
                    if proposal.header().id == msg.proposal {
                        // TODO: Leader is faulty? what should we do?
                        Some(proposal)
                    } else {
                        None
                    }
                }
            });
        let mut stream = Box::pin(stream);
        if let Some(block) = stream.next().await {
            Event::Proposal { block, stream }
        } else {
            Event::None
        }
    }

    fn update_leader_selection<
        E: std::error::Error,
        Fl: FnOnce(O::LeaderSelection) -> Result<O::LeaderSelection, E>,
    >(
        carnot: Carnot<O>,
        leader_selection_f: Fl,
    ) -> Carnot<O> {
        carnot
            .update_overlay(|overlay| overlay.update_leader_selection(leader_selection_f))
            .unwrap()
    }

    fn update_committee_membership<
        E: std::error::Error,
        Fm: FnOnce(O::CommitteeMembership) -> Result<O::CommitteeMembership, E>,
    >(
        carnot: Carnot<O>,
        committee_membership_f: Fm,
    ) -> Carnot<O> {
        carnot
            .update_overlay(|overlay| overlay.update_committees(committee_membership_f))
            .unwrap()
    }

    fn update_overlay<
        El: std::error::Error,
        Em: std::error::Error,
        Fl: FnOnce(O::LeaderSelection) -> Result<O::LeaderSelection, El>,
        Fm: FnOnce(O::CommitteeMembership) -> Result<O::CommitteeMembership, Em>,
    >(
        carnot: Carnot<O>,
        leader_selection_f: Fl,
        committee_membership_f: Fm,
    ) -> Carnot<O> {
        let carnot = Self::update_leader_selection(carnot, leader_selection_f);
        Self::update_committee_membership(carnot, committee_membership_f)
    }
}

async fn handle_output<A, Tx, C>(adapter: &A, node_id: NodeId, output: Output<Tx, C>)
where
    A: NetworkAdapter,
    Tx: Hash + Eq + Clone + Serialize + DeserializeOwned + Debug,
    C: Clone + Eq + Hash + Serialize + DeserializeOwned,
{
    match output {
        Output::Send(consensus_engine::Send { to, payload }) => match payload {
            Payload::Vote(vote) => {
                adapter
                    .send(
                        NetworkMessage::Vote(VoteMsg {
                            voter: node_id,
                            vote,
                            qc: None, // TODO: handle root commmittee members
                        }),
                        &to,
                    )
                    .await;
            }
            Payload::Timeout(timeout) => {
                adapter
                    .send(
                        NetworkMessage::Timeout(TimeoutMsg {
                            voter: node_id,
                            vote: timeout,
                        }),
                        &to,
                    )
                    .await;
            }
            Payload::NewView(new_view) => {
                adapter
                    .send(
                        NetworkMessage::NewView(NewViewMsg {
                            voter: node_id,
                            vote: new_view,
                        }),
                        &to,
                    )
                    .await;
            }
        },
        Output::BroadcastProposal { proposal } => {
            adapter
                .broadcast(NetworkMessage::Proposal(ProposalMsg {
                    proposal: proposal.header().id,
                    data: proposal.as_bytes().to_vec().into_boxed_slice(),
                    view: proposal.header().view,
                }))
                .await;
        }
        Output::BroadcastTimeoutQc { timeout_qc } => {
            adapter
                .broadcast(NetworkMessage::TimeoutQc(TimeoutQcMsg {
                    source: node_id,
                    qc: timeout_qc,
                }))
                .await;
        }
    }
}

#[allow(clippy::large_enum_variant)]
enum Event<Tx: Clone + Hash + Eq, BlobCertificate: Clone + Eq + Hash> {
    Proposal {
        block: Block<Tx, BlobCertificate>,
        stream: Pin<Box<dyn Stream<Item = Block<Tx, BlobCertificate>> + Send>>,
    },
    #[allow(dead_code)]
    Approve {
        qc: Qc,
        block: consensus_engine::Block,
        votes: HashSet<Vote>,
    },
    LocalTimeout {
        view: View,
    },
    NewView {
        timeout_qc: TimeoutQc,
        new_views: HashSet<NewView>,
    },
    TimeoutQc {
        timeout_qc: TimeoutQc,
    },
    RootTimeout {
        timeouts: HashSet<Timeout>,
    },
    ProposeBlock {
        qc: Qc,
    },
    None,
}

#[derive(Debug)]
pub enum ConsensusMsg {
    Info { tx: Sender<CarnotInfo> },
}

impl RelayMessage for ConsensusMsg {}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CarnotInfo {
    pub id: NodeId,
    pub current_view: View,
    pub highest_voted_view: View,
    pub local_high_qc: StandardQc,
    #[serde_as(as = "Vec<(_, _)>")]
    pub safe_blocks: HashMap<BlockId, consensus_engine::Block>,
    pub last_view_timeout_qc: Option<TimeoutQc>,
    pub committed_blocks: Vec<BlockId>,
}

async fn get_mempool_contents<Item, Key>(
    mempool: OutboundRelay<MempoolMsg<Item, Key>>,
) -> Result<Box<dyn Iterator<Item = Item> + Send>, tokio::sync::oneshot::error::RecvError> {
    let (reply_channel, rx) = tokio::sync::oneshot::channel();

    mempool
        .send(MempoolMsg::View {
            ancestor_hint: BlockId::zeros(),
            reply_channel,
        })
        .await
        .unwrap_or_else(|(e, _)| eprintln!("Could not get transactions from mempool {e}"));

    rx.await
}

#[cfg(test)]
mod tests {
    use consensus_engine::Block;

    use super::*;

    #[test]
    fn serde_carnot_info() {
        let info = CarnotInfo {
            id: NodeId::new([0; 32]),
            current_view: View::new(1),
            highest_voted_view: View::new(-1),
            local_high_qc: StandardQc {
                view: View::new(0),
                id: BlockId::zeros(),
            },
            safe_blocks: HashMap::from([(
                BlockId::zeros(),
                Block {
                    id: BlockId::zeros(),
                    view: View::new(0),
                    parent_qc: Qc::Standard(StandardQc {
                        view: View::new(0),
                        id: BlockId::zeros(),
                    }),
                    leader_proof: LeaderProof::LeaderId {
                        leader_id: NodeId::new([0; 32]),
                    },
                },
            )]),
            last_view_timeout_qc: None,
            committed_blocks: vec![BlockId::zeros()],
        };

        let serialized = serde_json::to_string(&info).unwrap();
        eprintln!("{serialized}");
        assert_eq!(
            serialized,
            r#"{"id":"0x0000000000000000000000000000000000000000000000000000000000000000","current_view":1,"highest_voted_view":-1,"local_high_qc":{"view":0,"id":"0x0000000000000000000000000000000000000000000000000000000000000000"},"safe_blocks":[["0x0000000000000000000000000000000000000000000000000000000000000000",{"view":0,"parent_qc":{"Standard":{"view":0,"id":"0x0000000000000000000000000000000000000000000000000000000000000000"}},"leader_proof":{"LeaderId":{"leader_id":"0x0000000000000000000000000000000000000000000000000000000000000000"}}}]],"last_view_timeout_qc":null,"committed_blocks":["0x0000000000000000000000000000000000000000000000000000000000000000"]}"#
        );

        let deserialized: CarnotInfo = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, info);
    }
}

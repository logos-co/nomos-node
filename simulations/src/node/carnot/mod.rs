#![allow(dead_code)]

mod event_builder;
mod message_cache;
pub mod messages;
mod tally;
mod timeout;

// std
use std::hash::Hash;
use std::time::Instant;
use std::{collections::HashMap, time::Duration};
// crates
use bls_signatures::PrivateKey;
use rand::Rng;
use serde::{Deserialize, Serialize};
// internal
use self::messages::CarnotMessage;
use super::{Node, NodeId};
use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};
use crate::node::carnot::event_builder::{CarnotTx, Event};
use crate::node::carnot::message_cache::MessageCache;
use consensus_engine::overlay::RandomBeaconState;
use consensus_engine::{
    Block, BlockId, Carnot, Committee, Overlay, Payload, Qc, StandardQc, TimeoutQc, View, Vote,
};
use nomos_consensus::committee_membership::UpdateableCommitteeMembership;
use nomos_consensus::network::messages::{ProposalChunkMsg, TimeoutQcMsg};
use nomos_consensus::{
    leader_selection::UpdateableLeaderSelection,
    network::messages::{NewViewMsg, TimeoutMsg, VoteMsg},
};

const NODE_ID: &str = "node_id";
const CURRENT_VIEW: &str = "current_view";
const HIGHEST_VOTED_VIEW: &str = "highest_voted_view";
const LOCAL_HIGH_QC: &str = "local_high_qc";
const SAFE_BLOCKS: &str = "safe_blocks";
const LAST_VIEW_TIMEOUT_QC: &str = "last_view_timeout_qc";
const LATEST_COMMITTED_BLOCK: &str = "latest_committed_block";
const LATEST_COMMITTED_VIEW: &str = "latest_committed_view";
const ROOT_COMMITTEE: &str = "root_committee";
const PARENT_COMMITTEE: &str = "parent_committee";
const CHILD_COMMITTEES: &str = "child_committees";
const COMMITTED_BLOCKS: &str = "committed_blocks";
const STEP_DURATION: &str = "step_duration";

pub const CARNOT_RECORD_KEYS: &[&str] = &[
    NODE_ID,
    CURRENT_VIEW,
    HIGHEST_VOTED_VIEW,
    LOCAL_HIGH_QC,
    SAFE_BLOCKS,
    LAST_VIEW_TIMEOUT_QC,
    LATEST_COMMITTED_BLOCK,
    LATEST_COMMITTED_VIEW,
    ROOT_COMMITTEE,
    PARENT_COMMITTEE,
    CHILD_COMMITTEES,
    COMMITTED_BLOCKS,
    STEP_DURATION,
];

static RECORD_SETTINGS: std::sync::OnceLock<HashMap<String, bool>> = std::sync::OnceLock::new();

#[derive(Debug)]
pub struct CarnotState {
    node_id: NodeId,
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc,
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
    latest_committed_block: Block,
    latest_committed_view: View,
    root_committee: Committee,
    parent_committee: Option<Committee>,
    child_committees: Vec<Committee>,
    committed_blocks: Vec<BlockId>,
    step_duration: Duration,
}

impl serde::Serialize for CarnotState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        if let Some(rs) = RECORD_SETTINGS.get() {
            let keys = rs
                .iter()
                .filter_map(|(k, v)| {
                    if CARNOT_RECORD_KEYS.contains(&k.trim()) && *v {
                        Some(k)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            let mut ser = serializer.serialize_struct("CarnotState", keys.len())?;
            for k in keys {
                match k.trim() {
                    NODE_ID => ser.serialize_field(NODE_ID, &self.node_id)?,
                    CURRENT_VIEW => ser.serialize_field(CURRENT_VIEW, &self.current_view)?,
                    HIGHEST_VOTED_VIEW => {
                        ser.serialize_field(HIGHEST_VOTED_VIEW, &self.highest_voted_view)?
                    }
                    LOCAL_HIGH_QC => ser.serialize_field(LOCAL_HIGH_QC, &self.local_high_qc)?,
                    SAFE_BLOCKS => {
                        #[derive(Serialize)]
                        #[serde(transparent)]
                        struct SafeBlockHelper<'a> {
                            #[serde(serialize_with = "serialize_blocks")]
                            safe_blocks: &'a HashMap<BlockId, Block>,
                        }
                        ser.serialize_field(
                            SAFE_BLOCKS,
                            &SafeBlockHelper {
                                safe_blocks: &self.safe_blocks,
                            },
                        )?;
                    }
                    LAST_VIEW_TIMEOUT_QC => {
                        ser.serialize_field(LAST_VIEW_TIMEOUT_QC, &self.last_view_timeout_qc)?
                    }
                    LATEST_COMMITTED_BLOCK => {
                        ser.serialize_field(LATEST_COMMITTED_BLOCK, &self.latest_committed_block)?
                    }
                    LATEST_COMMITTED_VIEW => {
                        ser.serialize_field(LATEST_COMMITTED_VIEW, &self.latest_committed_view)?
                    }
                    ROOT_COMMITTEE => ser.serialize_field(ROOT_COMMITTEE, &self.root_committee)?,
                    PARENT_COMMITTEE => {
                        ser.serialize_field(PARENT_COMMITTEE, &self.parent_committee)?
                    }
                    CHILD_COMMITTEES => {
                        ser.serialize_field(CHILD_COMMITTEES, &self.child_committees)?
                    }
                    COMMITTED_BLOCKS => {
                        ser.serialize_field(COMMITTED_BLOCKS, &self.committed_blocks)?
                    }
                    STEP_DURATION => ser.serialize_field(STEP_DURATION, &self.step_duration)?,
                    _ => {}
                }
            }
            ser.end()
        } else {
            serializer.serialize_none()
        }
    }
}

impl CarnotState {
    const fn keys() -> &'static [&'static str] {
        CARNOT_RECORD_KEYS
    }
}

/// Have to implement this manually because of the `serde_json` will panic if the key of map
/// is not a string.
fn serialize_blocks<S>(blocks: &HashMap<BlockId, Block>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut ser = serializer.serialize_map(Some(blocks.len()))?;
    for (k, v) in blocks {
        ser.serialize_entry(&format!("{k:?}"), v)?;
    }
    ser.end()
}

impl<O: Overlay> From<&Carnot<O>> for CarnotState {
    fn from(value: &Carnot<O>) -> Self {
        let node_id = value.id();
        let current_view = value.current_view();
        Self {
            node_id,
            current_view,
            local_high_qc: value.high_qc(),
            parent_committee: value.parent_committee(),
            root_committee: value.root_committee(),
            child_committees: value.child_committees(),
            latest_committed_block: value.latest_committed_block(),
            latest_committed_view: value.latest_committed_view(),
            safe_blocks: value
                .blocks_in_view(current_view)
                .into_iter()
                .map(|b| (b.id, b))
                .collect(),
            last_view_timeout_qc: value.last_view_timeout_qc(),
            committed_blocks: value.latest_committed_blocks(),
            highest_voted_view: Default::default(),
            step_duration: Default::default(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {
    timeout: Duration,
    record_settings: HashMap<String, bool>,
}

impl CarnotSettings {
    pub fn new(timeout: Duration, record_settings: HashMap<String, bool>) -> Self {
        Self {
            timeout,
            record_settings,
        }
    }
}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode<O: Overlay> {
    id: consensus_engine::NodeId,
    state: CarnotState,
    settings: CarnotSettings,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    message_cache: MessageCache,
    event_builder: event_builder::EventBuilder,
    engine: Carnot<O>,
    random_beacon_pk: PrivateKey,
    step_duration: Duration,
}

impl<
        L: UpdateableLeaderSelection,
        M: UpdateableCommitteeMembership,
        O: Overlay<LeaderSelection = L, CommitteeMembership = M>,
    > CarnotNode<O>
{
    pub fn new<R: Rng>(
        id: consensus_engine::NodeId,
        settings: CarnotSettings,
        overlay_settings: O::Settings,
        genesis: nomos_core::block::Block<CarnotTx>,
        network_interface: InMemoryNetworkInterface<CarnotMessage>,
        rng: &mut R,
    ) -> Self {
        let overlay = O::new(overlay_settings);
        let engine = Carnot::from_genesis(id, genesis.header().clone(), overlay);
        let state = CarnotState::from(&engine);
        let timeout = settings.timeout;
        RECORD_SETTINGS.get_or_init(|| settings.record_settings.clone());
        // pk is generated in an insecure way, but for simulation purpouses using a rng like smallrng is more useful
        let mut pk_buff = [0; 32];
        rng.fill_bytes(&mut pk_buff);
        let random_beacon_pk = PrivateKey::new(pk_buff);
        let mut this = Self {
            id,
            state,
            settings,
            network_interface,
            message_cache: MessageCache::new(),
            event_builder: event_builder::EventBuilder::new(id, timeout),
            engine,
            random_beacon_pk,
            step_duration: Duration::ZERO,
        };
        this.state = CarnotState::from(&this.engine);
        this
    }

    fn handle_output(&self, output: Output<CarnotTx>) {
        match output {
            Output::Send(consensus_engine::Send {
                to,
                payload: Payload::Vote(vote),
            }) => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::Vote(VoteMsg {
                            voter: self.id,
                            vote: vote.clone(),
                            qc: Some(Qc::Standard(StandardQc {
                                view: vote.view,
                                id: vote.block,
                            })),
                        }),
                    );
                }
            }
            Output::Send(consensus_engine::Send {
                to,
                payload: Payload::NewView(new_view),
            }) => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::NewView(NewViewMsg {
                            voter: node,
                            vote: new_view.clone(),
                        }),
                    );
                }
            }
            Output::Send(consensus_engine::Send {
                to,
                payload: Payload::Timeout(timeout),
            }) => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::Timeout(TimeoutMsg {
                            voter: node,
                            vote: timeout.clone(),
                        }),
                    );
                }
            }
            Output::BroadcastTimeoutQc { timeout_qc } => {
                self.network_interface
                    .broadcast(CarnotMessage::TimeoutQc(TimeoutQcMsg {
                        source: self.id,
                        qc: timeout_qc,
                    }));
            }
            Output::BroadcastProposal { proposal } => {
                self.network_interface
                    .broadcast(CarnotMessage::Proposal(ProposalChunkMsg {
                        chunk: proposal.as_bytes().to_vec().into(),
                        proposal: proposal.header().id,
                        view: proposal.header().view,
                    }))
            }
        }
    }

    fn process_event(&mut self, event: Event<[u8; 32]>) {
        let mut output = None;
        match event {
            Event::Proposal { block } => {
                let current_view = self.engine.current_view();
                tracing::info!(
                    node=%self.id,
                    last_committed_view=%self.engine.latest_committed_view(),
                    current_view = %current_view,
                    block_view = %block.header().view,
                    block = %block.header().id,
                    parent_block=%block.header().parent(),
                    "receive block proposal",
                );
                match self.engine.receive_block(block.header().clone()) {
                    Ok(new) => {
                        if self.engine.current_view() != new.current_view() {
                            // TODO: Refactor this into a method, use for timeout qc as well
                            // new = new
                            //     .update_overlay(|overlay| {
                            //         let overlay = overlay
                            //             .update_leader_selection(|leader_selection| {
                            //                 leader_selection.on_new_block_received(&block)
                            //             })
                            //             .expect("Leader selection update should succeed");
                            //         overlay.update_committees(|committee_membership| {
                            //             committee_membership.on_new_block_received(&block)
                            //         })
                            //     })
                            //     .unwrap_or(new);
                            self.engine = new;
                        }
                    }
                    Err(_) => {
                        tracing::error!(
                            node = %self.id,
                            current_view = %self.engine.current_view(),
                            block_view = %block.header().view, block = %block.header().id,
                            "receive block proposal, but is invalid",
                        );
                    }
                }

                if self.engine.overlay().is_member_of_leaf_committee(self.id) {
                    // Check if we are also a member of the parent committee, this is a special case for the flat committee
                    let to = if self.engine.overlay().is_member_of_root_committee(self.id) {
                        [self.engine.overlay().next_leader()].into_iter().collect()
                    } else {
                        self.engine.parent_committee().expect(
                            "Parent committee of non root committee members should be present",
                        )
                    };
                    output = Some(Output::Send(consensus_engine::Send {
                        to,
                        payload: Payload::Vote(Vote {
                            view: self.engine.current_view(),
                            block: block.header().id,
                        }),
                    }))
                }
            }
            // This branch means we already get enough votes for this block
            // So we can just call approve_block
            Event::Approve { block, .. } => {
                tracing::info!(
                    node = %self.id,
                    current_view = %self.engine.current_view(),
                    block_view = %block.view,
                    block = %block.id,
                    parent_block=%block.parent(),
                    "receive approve message"
                );
                let block_grandparent_view = match &block.parent_qc {
                    Qc::Standard(qc) => qc.view,
                    Qc::Aggregated(qc) => {
                        self.engine
                            .safe_blocks()
                            .get(&qc.high_qc.id)
                            .expect("Parent block must be present")
                            .view
                    }
                } - View::new(3);
                let (mut new, out) = self.engine.approve_block(block);
                tracing::info!(vote=?out, node=%self.id);
                // pruning old blocks older than the grandparent block needed to check validity
                new.prune_older_blocks_by_view(block_grandparent_view);
                output = Some(Output::Send(out));
                self.engine = new;
            }
            Event::ProposeBlock { qc } => {
                output = Some(Output::BroadcastProposal {
                    proposal: nomos_core::block::Block::new(
                        qc.view().next(),
                        qc.clone(),
                        [].into_iter(),
                        self.id,
                        RandomBeaconState::generate_happy(qc.view().next(), &self.random_beacon_pk),
                    ),
                });
            }
            // This branch means we already get enough new view msgs for this qc
            // So we can just call approve_new_view
            Event::NewView {
                timeout_qc,
                new_views,
            } => {
                tracing::info!(
                    node = %self.id,
                    current_view = %self.engine.current_view(),
                    timeout_view = %timeout_qc.view(),
                    "receive new view message"
                );
                // just process timeout if node have not already process it
                if timeout_qc.view() == self.engine.current_view() {
                    let (new, out) = self.engine.approve_new_view(timeout_qc, new_views);
                    output = Some(Output::Send(out));
                    self.engine = new;
                }
            }
            Event::TimeoutQc { timeout_qc } => {
                tracing::info!(
                    node = %self.id,
                    current_view = %self.engine.current_view(),
                    timeout_view = %timeout_qc.view(),
                    "receive timeout qc message"
                );
                self.engine = self.engine.receive_timeout_qc(timeout_qc);
            }
            Event::RootTimeout { timeouts } => {
                tracing::debug!("root timeout {:?}", timeouts);
                if self.engine.is_member_of_root_committee() {
                    assert!(timeouts
                        .iter()
                        .all(|t| t.view == self.engine.current_view()));
                    let high_qc = timeouts
                        .iter()
                        .map(|t| &t.high_qc)
                        .chain(std::iter::once(&self.engine.high_qc()))
                        .max_by_key(|qc| qc.view)
                        .expect("empty root committee")
                        .clone();
                    let timeout_qc =
                        TimeoutQc::new(timeouts.iter().next().unwrap().view, high_qc, self.id);
                    output = Some(Output::BroadcastTimeoutQc { timeout_qc });
                }
            }
            Event::LocalTimeout => {
                tracing::info!(
                    node = %self.id,
                    current_view = %self.engine.current_view(),
                    "receive local timeout message"
                );
                let (new, out) = self.engine.local_timeout();
                self.engine = new;
                output = out.map(Output::Send);
            }
            Event::None => {
                tracing::error!("unimplemented none branch");
                unreachable!("none event will never be constructed")
            }
        }
        if let Some(event) = output {
            self.handle_output(event);
        }
    }
}

impl<
        L: UpdateableLeaderSelection,
        M: UpdateableCommitteeMembership,
        O: Overlay<LeaderSelection = L, CommitteeMembership = M>,
    > Node for CarnotNode<O>
{
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> View {
        self.engine.current_view()
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self, elapsed: Duration) {
        let step_duration = Instant::now();

        // split messages per view, we just want to process the current engine processing view or proposals or timeoutqcs
        let (mut current_view_messages, other_view_messages): (Vec<_>, Vec<_>) = self
            .network_interface
            .receive_messages()
            .into_iter()
            .map(NetworkMessage::into_payload)
            // do not care for older view messages
            .filter(|m| m.view() >= self.engine.current_view())
            .partition(|m| {
                m.view() == self.engine.current_view()
                    || matches!(m, CarnotMessage::Proposal(_) | CarnotMessage::TimeoutQc(_))
            });
        self.message_cache.prune(self.engine.current_view().prev());
        self.message_cache.update(other_view_messages);
        current_view_messages.append(&mut self.message_cache.retrieve(self.engine.current_view()));

        let events = self
            .event_builder
            .step(current_view_messages, &self.engine, elapsed);

        for event in events {
            self.process_event(event);
        }

        // update state
        self.state = CarnotState::from(&self.engine);
        self.state.step_duration = step_duration.elapsed();
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Output<Tx: Clone + Eq + Hash> {
    Send(consensus_engine::Send),
    BroadcastTimeoutQc {
        timeout_qc: TimeoutQc,
    },
    BroadcastProposal {
        proposal: nomos_core::block::Block<Tx>,
    },
}

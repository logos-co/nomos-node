#![allow(dead_code)]

mod event_builder;
mod message_cache;
pub mod messages;
mod tally;
mod timeout;

// std
use std::hash::Hash;
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
use nomos_consensus::network::messages::{ProposalChunkMsg, TimeoutQcMsg};
use nomos_consensus::{
    leader_selection::UpdateableLeaderSelection,
    network::messages::{NewViewMsg, TimeoutMsg, VoteMsg},
};

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

pub const CARNOT_RECORD_KEYS: &[&str] = &[
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
];

static RECORD_SETTINGS: std::sync::OnceLock<HashMap<String, bool>> = std::sync::OnceLock::new();

pub struct CarnotState {
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc,
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
    latest_committed_block: Block,
    latest_committed_view: View,
    root_committe: Committee,
    parent_committe: Committee,
    child_committees: Vec<Committee>,
    committed_blocks: Vec<BlockId>,
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
                    ROOT_COMMITTEE => ser.serialize_field(ROOT_COMMITTEE, &self.root_committe)?,
                    PARENT_COMMITTEE => {
                        ser.serialize_field(PARENT_COMMITTEE, &self.parent_committe)?
                    }
                    CHILD_COMMITTEES => {
                        ser.serialize_field(CHILD_COMMITTEES, &self.child_committees)?
                    }
                    COMMITTED_BLOCKS => {
                        ser.serialize_field(COMMITTED_BLOCKS, &self.committed_blocks)?
                    }
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
        let current_view = value.current_view();
        Self {
            current_view,
            local_high_qc: value.high_qc(),
            parent_committe: value.parent_committee(),
            root_committe: value.root_committee(),
            child_committees: value.child_committees(),
            latest_committed_block: value.latest_committed_block(),
            latest_committed_view: value.latest_committed_view(),
            safe_blocks: value
                .blocks_in_view(current_view)
                .into_iter()
                .map(|b| (b.id, b))
                .collect(),
            last_view_timeout_qc: value.last_view_timeout_qc(),
            committed_blocks: value.committed_blocks(),
            highest_voted_view: Default::default(),
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
}

impl<O: Overlay> CarnotNode<O> {
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
}

impl<L: UpdateableLeaderSelection, O: Overlay<LeaderSelection = L>> Node for CarnotNode<O> {
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        let view: i64 = self.event_builder.current_view.into();
        view as usize
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self, elapsed: Duration) {
        // split messages per view, we just one to process the current engine processing view or proposals or timeoutqcs
        let (mut current_view_messages, other_view_messages): (Vec<_>, Vec<_>) = self
            .network_interface
            .receive_messages()
            .into_iter()
            .map(NetworkMessage::get_payload)
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
            let mut output: Vec<Output<CarnotTx>> = vec![];
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
                        Ok(mut new) => {
                            if self.engine.current_view() != new.current_view() {
                                new = new
                                    .update_overlay(|overlay| {
                                        overlay.update_leader_selection(|leader_selection| {
                                            leader_selection.on_new_block_received(block.clone())
                                        })
                                    })
                                    .unwrap_or(new);
                                self.engine = new;
                            }
                        }
                        Err(_) => {
                            tracing::error!(node = %self.id, current_view = %self.engine.current_view(), block_view = %block.header().view, block = %block.header().id, "receive block proposal, but is invalid");
                        }
                    }

                    if self.engine.overlay().is_member_of_leaf_committee(self.id) {
                        output.push(Output::Send(consensus_engine::Send {
                            to: self.engine.parent_committee(),
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
                    let (new, out) = self.engine.approve_block(block);
                    tracing::info!(vote=?out, node=%self.id);
                    output = vec![Output::Send(out)];
                    self.engine = new;
                }
                Event::ProposeBlock { qc } => {
                    output = vec![Output::BroadcastProposal {
                        proposal: nomos_core::block::Block::new(
                            qc.view().next(),
                            qc.clone(),
                            [].into_iter(),
                            self.id,
                            RandomBeaconState::generate_happy(
                                qc.view().next(),
                                &self.random_beacon_pk,
                            ),
                        ),
                    }]
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
                    let (new, out) = self.engine.approve_new_view(timeout_qc.clone(), new_views);
                    output.push(Output::Send(out));
                    self.engine = new;
                }
                Event::TimeoutQc { timeout_qc } => {
                    tracing::info!(
                        node = %self.id,
                        current_view = %self.engine.current_view(),
                        timeout_view = %timeout_qc.view(),
                        "receive timeout qc message"
                    );
                    self.engine = self.engine.receive_timeout_qc(timeout_qc.clone());
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
                        let timeout_qc = TimeoutQc::new(
                            timeouts.iter().next().unwrap().view,
                            high_qc,
                            self.id(),
                        );
                        output.push(Output::BroadcastTimeoutQc { timeout_qc });
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
                    if let Some(out) = out {
                        output.push(Output::Send(out));
                    }
                }
                Event::None => {
                    tracing::error!("unimplemented none branch");
                    unreachable!("none event will never be constructed")
                }
            }

            for output_event in output {
                self.handle_output(output_event);
            }
        }

        // update state
        self.state = CarnotState::from(&self.engine);
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

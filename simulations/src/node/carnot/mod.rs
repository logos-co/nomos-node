#![allow(dead_code)]

mod event_builder;
mod message_cache;
pub mod messages;
mod state;
pub use state::*;
mod serde_util;
mod tally;
mod timeout;

use std::any::Any;
use std::collections::BTreeMap;
// std
use std::hash::Hash;
use std::time::Instant;
use std::{collections::HashMap, time::Duration};
// crates
use bls_signatures::PrivateKey;
use rand::Rng;
use serde::Deserialize;
// internal
use self::messages::CarnotMessage;
use super::{Node, NodeId};
use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};
use crate::node::carnot::event_builder::{CarnotBlob, CarnotTx, Event};
use crate::node::carnot::message_cache::MessageCache;
use crate::output_processors::{Record, RecordType, Runtime};
use crate::settings::SimulationSettings;
use crate::streaming::SubscriberFormat;
use crate::warding::SimulationState;
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

static RECORD_SETTINGS: std::sync::OnceLock<BTreeMap<String, bool>> = std::sync::OnceLock::new();

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {
    timeout: Duration,
    record_settings: BTreeMap<String, bool>,

    #[serde(default)]
    format: SubscriberFormat,
}

impl CarnotSettings {
    pub fn new(
        timeout: Duration,
        record_settings: BTreeMap<String, bool>,
        format: SubscriberFormat,
    ) -> Self {
        Self {
            timeout,
            record_settings,
            format,
        }
    }
}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode<O: Overlay> {
    id: consensus_engine::NodeId,
    state: CarnotState,
    /// A step counter
    current_step: usize,
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
        genesis: nomos_core::block::Block<CarnotTx, CarnotBlob>,
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
            current_step: 0,
        };
        this.state = CarnotState::from(&this.engine);
        this.state.format = this.settings.format;
        this
    }

    fn handle_output(&self, output: Output<CarnotTx, CarnotBlob>) {
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
                    Ok(mut new) => {
                        if self.engine.current_view() != new.current_view() {
                            new = Self::update_overlay_with_block(new, &block);
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
                let new = self.engine.receive_timeout_qc(timeout_qc.clone());
                self.engine = Self::update_overlay_with_timeout_qc(new, &timeout_qc);
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

    fn update_overlay_with_block<Tx: Clone + Eq + Hash>(
        state: Carnot<O>,
        block: &nomos_core::block::Block<Tx, CarnotBlob>,
    ) -> Carnot<O> {
        state
            .update_overlay(|overlay| {
                overlay
                    .update_leader_selection(|leader_selection| {
                        leader_selection.on_new_block_received(block)
                    })
                    .expect("Leader selection update should succeed")
                    .update_committees(|committee_membership| {
                        committee_membership.on_new_block_received(block)
                    })
            })
            .unwrap_or(state)
    }

    fn update_overlay_with_timeout_qc(state: Carnot<O>, qc: &TimeoutQc) -> Carnot<O> {
        state
            .update_overlay(|overlay| {
                overlay
                    .update_leader_selection(|leader_selection| {
                        leader_selection.on_timeout_qc_received(qc)
                    })
                    .expect("Leader selection update should succeed")
                    .update_committees(|committee_membership| {
                        committee_membership.on_timeout_qc_received(qc)
                    })
            })
            .unwrap_or(state)
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
        self.event_builder
            .prune_by_view(self.engine.current_view().prev());
        self.message_cache.update(other_view_messages);
        current_view_messages.append(&mut self.message_cache.retrieve(self.engine.current_view()));

        let events = self
            .event_builder
            .step(current_view_messages, &self.engine, elapsed);

        for event in events {
            self.process_event(event);
        }

        // update state
        self.state = CarnotState::new(
            self.current_step,
            step_duration.elapsed(),
            self.settings.format,
            &self.engine,
        );
        self.current_step += 1;
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum Output<Tx: Clone + Eq + Hash, Blob: Clone + Eq + Hash> {
    Send(consensus_engine::Send),
    BroadcastTimeoutQc {
        timeout_qc: TimeoutQc,
    },
    BroadcastProposal {
        proposal: nomos_core::block::Block<Tx, Blob>,
    },
}

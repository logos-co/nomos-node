mod event_builder;
mod messages;

use std::collections::{HashMap, HashSet};

use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};

// std
// crates
use self::{event_builder::EventBuilderSettings, messages::CarnotMessage, overlay::CarnotOverlay};
use consensus_engine::{
    Block, BlockId, Carnot, Committee, Overlay, Payload, Qc, StandardQc, TimeoutQc, View, Vote,
};
use nomos_consensus::{
    network::messages::{NewViewMsg, TimeoutMsg, VoteMsg},
    Event,
};
use serde::{Deserialize, Serialize};

// internal
use super::{Node, NodeId};

#[derive(Default, Serialize)]
pub struct CarnotState {
    current_view: View,
    local_high_qc: StandardQc,
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
    latest_committed_block: Block,
    latest_committed_view: View,
    root_committe: Committee,
    parent_committe: Committee,
    child_committee: Committee,
    committed_blocks: Vec<BlockId>,
}

impl<O: Overlay> From<&Carnot<O>> for CarnotState {
    fn from(value: &Carnot<O>) -> Self {
        let current_view = value.current_view();
        Self {
            current_view,
            local_high_qc: value.high_qc(),
            parent_committe: value.parent_committee(),
            root_committe: value.root_committee(),
            child_committee: value.child_committee(),
            latest_committed_block: value.latest_committed_block(),
            latest_committed_view: value.latest_committed_view(),
            safe_blocks: value.blocks_in_view(current_view),
            last_view_timeout_qc: value.last_view_timeout_qc(),
            committed_blocks: value.committed_blocks(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {
    nodes: Vec<NodeId>,
    seed: u64,
}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode<O: Overlay> {
    id: NodeId,
    state: CarnotState,
    settings: CarnotSettings,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    event_builder: event_builder::EventBuilder,
    engine: Carnot<O>,
}

impl<O: Overlay> CarnotNode<O> {
    pub fn new(id: NodeId, settings: CarnotSettings) -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        let genesis = consensus_engine::Block {
            id: [0; 32],
            view: 0,
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        let overlay = O::new(settings.nodes.clone());
        Self {
            id,
            state: Default::default(),
            settings,
            network_interface: InMemoryNetworkInterface::new(id, sender, receiver),
            event_builder: Default::default(),
            engine: Carnot::from_genesis(id, genesis, overlay),
        }
    }

    pub fn send_message(&self, message: NetworkMessage<CarnotMessage>) {
        self.network_interface.send_message(self.id, message);
    }

    pub fn approve_block(&self, block: Block) -> consensus_engine::Send {
        assert!(
            self.state.safe_blocks.contains_key(&block.id),
            "{:?} not in {:?}",
            block,
            self.state.safe_blocks
        );
        assert!(
            self.state.highest_voted_view < block.view,
            "can't vote for a block in the past"
        );

        // update state
        self.state.highest_voted_view = block.view;

        let to = if self.overlay.is_member_of_root_committee(self.id) {
            [self.overlay.leader(block.view + 1)].into_iter().collect()
        } else {
            self.overlay.parent_committee(self.id)
        };

        consensus_engine::Send {
            to,
            payload: Payload::Vote(Vote {
                block: block.id,
                view: block.view,
            }),
        }
    }

    fn is_leader_for_view(&self, view: View) -> bool {
        self.overlay.leader(view) == self.id
    }

    fn gather_votes(&self, committee: &Committee, block: consensus_engine::Block) {
        use rand::seq::IteratorRandom;
        use rand::Rng;
        // random choose some commitees to vote
        for node in committee.iter().choose_multiple(
            &mut rand::thread_rng(),
            self.event_builder.config.votes_threshold,
        ) {
            self.network_interface.send_message(
                *node,
                CarnotMessage::Vote(VoteMsg {
                    voter: *node,
                    vote: Vote {
                        block: block.id,
                        view: block.view,
                    },
                    qc: Some(block.parent_qc),
                }),
            );
        }
    }

    fn gather_new_views(&self, committee: &Committee, timeout_qc: TimeoutQc) {
        use rand::seq::IteratorRandom;
        use rand::Rng;

        // random choose some commitees to new view
        for node in committee.iter().choose_multiple(
            &mut rand::thread_rng(),
            self.event_builder.config.new_view_threadhold,
        ) {
            self.network_interface.send_message(
                *node,
                CarnotMessage::Vote(NewViewMsg {
                    voter: *node,
                    vote: Vote {
                        block: block.id,
                        view: block.view,
                    },
                }),
            );
        }
    }

    fn handle_output(&self, output: consensus_engine::Send) {
        match output {
            consensus_engine::Send {
                to,
                payload: Payload::Vote(vote),
            } => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::Vote(VoteMsg {
                            voter: self.id,
                            vote,
                            qc: None,
                        }),
                    );
                }
            }
            consensus_engine::Send {
                to,
                payload: Payload::NewView(new_view),
            } => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::NewView(NewViewMsg {
                            voter: node,
                            vote: new_view,
                        }),
                    );
                }
            }
            consensus_engine::Send {
                to,
                payload: Payload::Timeout(timeout),
            } => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::Timeout(TimeoutMsg {
                            voter: node,
                            vote: timeout,
                        }),
                    );
                }
            }
        }
    }
}

impl<O: Overlay> Node for CarnotNode<O> {
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.event_builder.current_view
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self) {
        let events = self.event_builder.step(
            self.network_interface
                .receive_messages()
                .into_iter()
                .map(|m| m.payload),
        );

        for event in events {
            let mut output = None;
            match event {
                Event::Proposal { block, stream } => {
                    println!("receive proposal {:?}", block.header().id);
                    match self.engine.receive_block(block) {
                        Ok(new) => {
                            let new_view = new.current_view();
                            if new_view != self.engine.current_view() {
                                self.gather_votes(&self.engine.child_committee(), block);
                            }
                            self.engine = new;
                        }
                        Err(_) => println!("invalid block {:?}", block),
                    }
                }
                // This branch means we already get enough votes for this block
                // So we can just call approve_block
                Event::Approve { qc, block, votes } => {
                    let (new, out) = self.engine.approve_block(block);
                    output = Some(out);
                    self.engine = new;
                }
                // This branch means we already get enough new view msgs for this qc
                // So we can just call approve_new_view
                Event::NewView {
                    timeout_qc,
                    new_views,
                } => {
                    let (new, out) = self.engine.approve_new_view(timeout_qc, new_views);
                    output = Some(out);
                    self.engine = new;
                    let next_view = timeout_qc.view + 2;
                    if self.engine.is_leader_for_view(next_view) {
                        self.gather_new_views(&[self.id].into_iter().collect(), timeout_qc);
                    }
                }
                Event::TimeoutQc { timeout_qc } => {
                    let child = self.engine.child_committee();
                    self.engine = self.engine.receive_timeout_qc(timeout_qc);
                    self.gather_new_views(&child, timeout_qc);
                }
                Event::RootTimeout { timeouts } => {
                    println!("root timeouts: {:?}", timeouts);
                }
                Event::ProposeBlock { .. } => {
                    unreachable!("propose block will never be constructed")
                }
                Event::LocalTimeout => unreachable!("local timeout will never be constructed"),
                Event::None => unreachable!("none event will never be constructed"),
            }

            if let Some(output) = output {
                self.handle_output(output);
            }
        }

        // update state
        self.state = CarnotState::from(&self.engine);
    }
}

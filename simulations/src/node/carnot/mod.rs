#![allow(dead_code)]

mod event_builder;
mod messages;

use std::{collections::HashMap, time::Duration};

use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};

// std
// crates
use self::messages::CarnotMessage;
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

#[derive(Serialize)]
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
    nodes: Vec<consensus_engine::NodeId>,
    seed: u64,
    timeout: Duration,
}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode<O: Overlay> {
    id: consensus_engine::NodeId,
    state: CarnotState,
    settings: CarnotSettings,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    event_builder: event_builder::EventBuilder,
    engine: Carnot<O>,
}

impl<O: Overlay> CarnotNode<O> {
    pub fn new(id: consensus_engine::NodeId, settings: CarnotSettings) -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        let genesis = consensus_engine::Block {
            id: [0; 32],
            view: 0,
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        let overlay = O::new(settings.nodes.clone());
        let engine = Carnot::from_genesis(id, genesis, overlay);
        let state = CarnotState::from(&engine);

        Self {
            id,
            state,
            settings,
            network_interface: InMemoryNetworkInterface::new(id, sender, receiver),
            event_builder: event_builder::EventBuilder::new(id),
            engine,
        }
    }

    pub(crate) fn send_message(&self, message: NetworkMessage<CarnotMessage>) {
        self.network_interface
            .send_message(self.id, message.payload);
    }

    pub fn approve_block(&mut self, block: Block) -> consensus_engine::Send {
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

        let to = if self.engine.is_member_of_root_committee() {
            [self.engine.leader(block.view + 1)].into_iter().collect()
        } else {
            self.engine.parent_committee()
        };

        consensus_engine::Send {
            to,
            payload: Payload::Vote(Vote {
                block: block.id,
                view: block.view,
            }),
        }
    }

    fn gather_votes(&self, committee: &Committee, block: consensus_engine::Block) {
        use rand::seq::IteratorRandom;
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
                    qc: Some(block.parent_qc.clone()),
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
                            vote: vote.clone(),
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
                            vote: new_view.clone(),
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
                            vote: timeout.clone(),
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
        self.event_builder.current_view as usize
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self) {
        let events = self.event_builder.step(
            self.network_interface
                .receive_messages()
                .into_iter()
                .map(|m| m.payload)
                .collect(),
            &self.engine,
        );

        for event in events {
            let mut output = None;
            match event {
                Event::Proposal { block, stream: _ } => {
                    println!("receive proposal {:?}", block.header().id);
                    match self.engine.receive_block(consensus_engine::Block {
                        id: block.header().id,
                        view: block.header().view,
                        parent_qc: block.header().parent_qc.clone(),
                    }) {
                        Ok(new) => self.engine = new,
                        Err(_) => println!("invalid block {:?}", block),
                    }
                }
                // This branch means we already get enough votes for this block
                // So we can just call approve_block
                Event::Approve {
                    qc: _,
                    block,
                    votes: _,
                } => {
                    let (new, out) = self.engine.approve_block(block);
                    output = Some(out);
                    self.engine = new;
                }
                // This branch means we already get enough new view msgs for this qc
                // So we can just call approve_new_view
                Event::NewView {
                    timeout_qc: _,
                    new_views: _,
                } => {
                    // let (new, out) = self.engine.approve_new_view(timeout_qc, new_views);
                    // output = Some(out);
                    // self.engine = new;
                    // let next_view = timeout_qc.view + 2;
                    // if self.engine.is_leader_for_view(next_view) {
                    //     self.gather_new_views(&[self.id].into_iter().collect(), timeout_qc);
                    // }
                    unimplemented!()
                }
                Event::TimeoutQc { timeout_qc } => {
                    self.engine = self.engine.receive_timeout_qc(timeout_qc);
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

#[test]
fn test_() {}

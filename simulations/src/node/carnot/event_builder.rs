use crate::node::carnot::messages::CarnotMessage;
use crate::node::carnot::tally::Tally;
use crate::node::carnot::timeout::TimeoutHandler;
use crate::util::parse_idx;
use consensus_engine::{
    AggregateQc, Carnot, NewView, Overlay, Qc, StandardQc, Timeout, TimeoutQc, View, Vote,
};
use nomos_consensus::network::messages::{NewViewMsg, TimeoutMsg, VoteMsg};
use nomos_consensus::NodeId;
use nomos_core::block::{Block, BlockId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::time::Duration;

pub type CarnotTx = [u8; 32];

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
pub struct EventBuilderSettings {
    pub votes_threshold: usize,
    pub timeout_threshold: usize,
    pub new_view_threshold: usize,
}

pub(crate) struct EventBuilder {
    id: NodeId,
    blocks: HashMap<BlockId, Block<CarnotTx>>,
    vote_message: Tally<VoteMsg>,
    leader_vote_message: Tally<VoteMsg>,
    timeout_message: Tally<TimeoutMsg>,
    new_view_message: Tally<NewViewMsg>,
    timeout_handler: TimeoutHandler,
    pub(crate) config: EventBuilderSettings,
    pub(crate) current_view: View,
}

impl EventBuilder {
    pub fn new(id: NodeId, genesis: Block<CarnotTx>, timeout: Duration) -> Self {
        Self {
            vote_message: Default::default(),
            leader_vote_message: Default::default(),
            timeout_message: Default::default(),
            config: Default::default(),
            blocks: [(genesis.header().id, genesis)].into_iter().collect(),
            new_view_message: Default::default(),
            current_view: View::default(),
            id,
            timeout_handler: TimeoutHandler::new(timeout),
        }
    }

    fn local_timeout(
        &mut self,
        view: View,
        member_of_root_committee: bool,
        elapsed: Duration,
    ) -> bool {
        if member_of_root_committee {
            if self.timeout_handler.step(view, elapsed) {
                self.timeout_handler.prune_by_view(view);
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn step<O: Overlay>(
        &mut self,
        messages: Vec<CarnotMessage>,
        engine: &Carnot<O>,
        elapsed: Duration,
    ) -> Vec<Event<CarnotTx>> {
        // check timeout and exit
        // TODO: probably a good place to prune old view messages
        if self.local_timeout(
            engine.current_view(),
            engine.is_member_of_root_committee(),
            elapsed,
        ) {
            return vec![Event::LocalTimeout];
        }

        let mut events = Vec::new();
        // only run when the engine is in the genesis view
        if engine.highest_voted_view() == -1
            && engine.overlay().is_member_of_leaf_committee(self.id)
        {
            tracing::info!(node = parse_idx(&self.id), "voting genesis",);
            let genesis = engine.genesis_block();
            events.push(Event::Approve {
                qc: Qc::Standard(StandardQc {
                    view: genesis.view,
                    id: genesis.id,
                }),
                block: genesis,
                votes: HashSet::new(),
            })
        }

        for message in messages {
            match message {
                CarnotMessage::Proposal(msg) => {
                    let block = Block::from_bytes(&msg.chunk);
                    if self.blocks.contains_key(&block.header().id) {
                        continue;
                    }
                    self.blocks.insert(block.header().id, block.clone());
                    tracing::info!(
                        node=parse_idx(&self.id),
                        leader=parse_idx(&engine.leader(block.header().view)),
                        current_view = engine.current_view(),
                        block_view=block.header().view,
                        block=?block.header().id,
                        parent_block=?block.header().parent(),
                        "receive proposal message",
                    );
                    events.push(Event::Proposal { block })
                }
                CarnotMessage::TimeoutQc(msg) => {
                    events.push(Event::TimeoutQc { timeout_qc: msg.qc });
                }
                CarnotMessage::Vote(msg) => {
                    let msg_view = msg.vote.view;
                    let block_id = msg.vote.block;
                    let voter = msg.voter;
                    let is_next_view_leader = engine.is_leader_for_view(msg_view + 1);
                    let is_current_view_leader = engine.is_leader_for_view(msg_view);
                    let tally = if engine.overlay().is_member_of_root_committee(voter)
                        && is_current_view_leader
                    {
                        &mut self.leader_vote_message
                    } else {
                        &mut self.vote_message
                    };

                    let Some(qc) = msg.qc.clone() else {
                        tracing::warn!(node=?parse_idx(&self.id), current_view = engine.current_view(), "received vote without QC");
                        continue;
                    };

                    // if we are the leader, then use the leader threshold, otherwise use the leaf threshold
                    let threshold = if is_current_view_leader {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };
                    if let Some(votes) = tally.tally_by(msg_view, msg, threshold) {
                        if let Some(block) =
                            self.blocks
                                .get(&block_id)
                                .cloned()
                                .map(|b| consensus_engine::Block {
                                    id: b.header().id,
                                    view: b.header().view,
                                    parent_qc: b.header().parent_qc.clone(),
                                })
                        {
                            tracing::info!(
                                node=parse_idx(&self.id),
                                leader=parse_idx(&engine.leader(msg_view)),
                                votes=votes.len(),
                                current_view = engine.current_view(),
                                block_view=block.view,
                                block=?block.id,
                                "approve block",
                            );

                            if is_next_view_leader {
                                events.push(Event::ProposeBlock {
                                    qc: Qc::Standard(StandardQc {
                                        view: block.view,
                                        id: block.id,
                                    }),
                                });
                            }

                            if is_current_view_leader {
                                events.push(Event::Approve {
                                    qc,
                                    block,
                                    votes: votes.into_iter().map(|v| v.vote).collect(),
                                });
                            }
                        }
                    }
                }
                CarnotMessage::Timeout(msg) => {
                    let msg_view = msg.vote.view;
                    let is_current_view_leader = engine.is_leader_for_view(msg_view);
                    let threshold = if is_current_view_leader {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };
                    if let Some(timeouts) = self.timeout_message.tally_by(msg_view, msg, threshold)
                    {
                        if engine.is_member_of_root_committee() {
                            events.push(Event::RootTimeout {
                                timeouts: timeouts.into_iter().map(|v| v.vote).collect(),
                            });
                        }
                    }
                }
                CarnotMessage::NewView(msg) => {
                    let msg_view = msg.vote.view;
                    let timeout_qc = msg.vote.timeout_qc.clone();
                    let is_leader = engine.is_leader_for_view(msg_view);
                    self.current_view = core::cmp::max(self.current_view, msg_view);
                    // if we are the leader, then use the leader threshold, otherwise use the leaf threshold
                    let threshold = if is_leader {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };

                    if let Some(new_views) =
                        self.new_view_message.tally_by(msg_view, msg, threshold)
                    {
                        if is_leader {
                            let high_qc = engine.high_qc();
                            events.push(Event::ProposeBlock {
                                qc: Qc::Aggregated(AggregateQc {
                                    high_qc,
                                    view: msg_view + 1,
                                }),
                            });
                            continue;
                        }

                        events.push(Event::NewView {
                            new_views: new_views.into_iter().map(|v| v.vote).collect(),
                            timeout_qc,
                        })
                    }
                }
            }
        }

        // check timeout
        if self.timeout_handler.is_timeout(engine.current_view())
            && engine.is_member_of_root_committee()
        {
            events.push(Event::LocalTimeout);
        }
        events
    }
}

pub enum Event<Tx: Clone + Hash + Eq> {
    Proposal {
        block: Block<Tx>,
    },
    #[allow(dead_code)]
    Approve {
        qc: Qc,
        block: consensus_engine::Block,
        votes: HashSet<Vote>,
    },
    ProposeBlock {
        qc: Qc,
    },
    LocalTimeout,
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
    None,
}

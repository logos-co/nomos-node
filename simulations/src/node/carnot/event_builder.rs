use crate::node::carnot::{messages::CarnotMessage, tally::Tally, timeout::TimeoutHandler};
use consensus_engine::{
    AggregateQc, Carnot, NewView, Overlay, Qc, StandardQc, Timeout, TimeoutQc, View, Vote,
};
use nomos_consensus::network::messages::{NewViewMsg, TimeoutMsg, VoteMsg};
use nomos_consensus::NodeId;
use nomos_core::block::Block;
use std::collections::HashSet;
use std::hash::Hash;
use std::time::Duration;

pub type CarnotTx = [u8; 32];

pub(crate) struct EventBuilder {
    id: NodeId,
    leader_vote_message: Tally<VoteMsg>,
    vote_message: Tally<VoteMsg>,
    timeout_message: Tally<TimeoutMsg>,
    leader_new_view_message: Tally<NewViewMsg>,
    new_view_message: Tally<NewViewMsg>,
    timeout_handler: TimeoutHandler,
    pub(crate) current_view: View,
}

impl EventBuilder {
    pub fn new(id: NodeId, timeout: Duration) -> Self {
        Self {
            vote_message: Default::default(),
            leader_vote_message: Default::default(),
            timeout_message: Default::default(),
            leader_new_view_message: Default::default(),
            new_view_message: Default::default(),
            current_view: View::default(),
            id,
            timeout_handler: TimeoutHandler::new(timeout),
        }
    }

    fn local_timeout(&mut self, view: View, elapsed: Duration) -> bool {
        if self.timeout_handler.step(view, elapsed) {
            self.timeout_handler.prune_by_view(view);
            true
        } else {
            false
        }
    }

    pub fn step<O: Overlay>(
        &mut self,
        mut messages: Vec<CarnotMessage>,
        engine: &Carnot<O>,
        elapsed: Duration,
    ) -> Vec<Event<CarnotTx>> {
        let mut events = Vec::new();
        // check timeout and exit
        if self.local_timeout(engine.current_view(), elapsed) {
            events.push(Event::LocalTimeout);
            // if we timeout discard incoming current view messages
            messages.retain(|msg| {
                matches!(
                    msg,
                    CarnotMessage::Proposal(_) | CarnotMessage::TimeoutQc(_)
                )
            });
        }

        // only run when the engine is in the genesis view
        if engine.highest_voted_view() == View::new(-1)
            && engine.overlay().is_member_of_leaf_committee(self.id)
        {
            tracing::info!(node = %self.id, "voting genesis",);
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
                    tracing::info!(
                        node=%self.id,
                        current_view = %engine.current_view(),
                        block_view=%block.header().view,
                        block=?block.header().id,
                        parent_block=?block.header().parent(),
                        "receive proposal message",
                    );
                    events.push(Event::Proposal { block })
                }
                CarnotMessage::TimeoutQc(msg) => {
                    let timeout_qc = msg.qc.clone();
                    events.push(Event::TimeoutQc { timeout_qc: msg.qc });
                    if engine.overlay().is_member_of_leaf_committee(self.id) {
                        events.push(Event::NewView {
                            timeout_qc,
                            new_views: Default::default(),
                        });
                    }
                }
                CarnotMessage::Vote(msg) => {
                    let msg_view = msg.vote.view;
                    let block_id = msg.vote.block;
                    let voter = msg.voter;
                    let is_next_view_leader = engine.is_next_leader();
                    let is_message_from_root_committee =
                        engine.overlay().is_member_of_root_committee(voter);

                    let tally = if is_message_from_root_committee {
                        &mut self.leader_vote_message
                    } else {
                        &mut self.vote_message
                    };

                    let Some(qc) = msg.qc.clone() else {
                        tracing::warn!(node=%self.id, current_view = %engine.current_view(), "received vote without QC");
                        continue;
                    };

                    // if the message comes from the root committee, then use the leader threshold, otherwise use the leaf threshold
                    let threshold = if is_message_from_root_committee {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };

                    if let Some(votes) = tally.tally_by(msg_view, msg, threshold) {
                        if let Some(block) = engine
                            .blocks_in_view(msg_view)
                            .iter()
                            .find(|block| block.id == block_id)
                            .cloned()
                        {
                            tracing::info!(
                                node=%self.id,
                                votes=votes.len(),
                                current_view = %engine.current_view(),
                                block_view=%block.view,
                                block=%block.id,
                                "approve block",
                            );

                            if is_next_view_leader && is_message_from_root_committee {
                                events.push(Event::ProposeBlock {
                                    qc: Qc::Standard(StandardQc {
                                        view: block.view,
                                        id: block.id,
                                    }),
                                });
                            } else {
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
                    if let Some(timeouts) = self.timeout_message.tally(msg_view, msg) {
                        events.push(Event::RootTimeout {
                            timeouts: timeouts.into_iter().map(|v| v.vote).collect(),
                        })
                    }
                }
                CarnotMessage::NewView(msg) => {
                    let msg_view = msg.vote.view;
                    let voter = msg.voter;
                    let timeout_qc = msg.vote.timeout_qc.clone();
                    let is_next_view_leader = engine.is_next_leader();
                    let is_message_from_root_committee =
                        engine.overlay().is_member_of_root_committee(voter);

                    let tally = if is_message_from_root_committee {
                        &mut self.leader_new_view_message
                    } else {
                        &mut self.new_view_message
                    };

                    // if the message comes from the root committee, then use the leader threshold, otherwise use the leaf threshold
                    let threshold = if is_message_from_root_committee {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };

                    if let Some(votes) = tally.tally_by(msg_view, msg, threshold) {
                        if is_next_view_leader && is_message_from_root_committee {
                            let high_qc = engine.high_qc();
                            events.push(Event::ProposeBlock {
                                qc: Qc::Aggregated(AggregateQc {
                                    high_qc,
                                    view: msg_view.next(),
                                }),
                            });
                        } else {
                            events.push(Event::NewView {
                                timeout_qc,
                                new_views: votes.into_iter().map(|v| v.vote).collect(),
                            });
                        }
                    }
                }
            }
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

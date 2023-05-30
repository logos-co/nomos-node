use crate::node::carnot::messages::CarnotMessage;
use crate::util::parse_idx;
use consensus_engine::{Carnot, Overlay, Qc, View};
use nomos_consensus::network::messages::{NewViewMsg, TimeoutMsg, VoteMsg};
use nomos_consensus::{Event, NodeId};
use nomos_core::block::{Block, BlockId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    timeout_message: Tally<TimeoutMsg>,
    new_view_message: Tally<NewViewMsg>,
    proposal_seen: HashSet<View>,
    pub(crate) config: EventBuilderSettings,
    pub(crate) current_view: View,
}

impl EventBuilder {
    pub fn new(id: NodeId) -> Self {
        Self {
            vote_message: Default::default(),
            timeout_message: Default::default(),
            config: Default::default(),
            blocks: Default::default(),
            new_view_message: Default::default(),
            proposal_seen: Default::default(),
            current_view: View::default(),
            id,
        }
    }

    pub fn step<O: Overlay>(
        &mut self,
        messages: Vec<CarnotMessage>,
        engine: &Carnot<O>,
    ) -> Vec<Event<CarnotTx>> {
        let mut events = Vec::new();
        self.try_handle_leader(engine, &mut events);
        for message in messages {
            match message {
                CarnotMessage::Proposal(msg) => {
                    let block = Block::from_bytes(&msg.chunk);
                    self.blocks.insert(block.header().id, block.clone());
                    events.push(Event::Proposal {
                        block,
                        stream: Box::pin(futures::stream::empty()),
                    });
                }
                CarnotMessage::TimeoutQc(msg) => {
                    events.push(Event::TimeoutQc { timeout_qc: msg.qc });
                }
                CarnotMessage::Vote(msg) => {
                    let msg_view = msg.vote.view;
                    let block_id = msg.vote.block;
                    let qc = msg.qc.clone().expect("empty QC from vote message");

                    // if we are the leader, then use the leader threshold, otherwise use the leaf threshold
                    let threshold = if engine.is_leader_for_view(msg_view) {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };

                    if let Some(votes) = self.vote_message.tally_by(msg_view, msg, threshold) {
                        
                        let block = self
                        .blocks
                        .get(&block_id)
                        .cloned()
                        .map(|b| consensus_engine::Block {
                            id: b.header().id,
                            view: b.header().view,
                            parent_qc: b.header().parent_qc.clone(),
                        })
                        .unwrap_or_else(|| panic!("cannot find block id {block_id:?}"));
                        println!("Node({}) approve: votes: {}, view: {}, block {:?}", parse_idx(&self.id), votes.len(), block.view, block.id);

                        events.push(Event::Approve {
                            qc,
                            block,
                            votes: votes.into_iter().map(|v| v.vote).collect(),
                        })
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
                    let timeout_qc = msg.vote.timeout_qc.clone();
                    self.current_view = core::cmp::max(self.current_view, msg_view);
                    // if we are the leader, then use the leader threshold, otherwise use the leaf threshold
                    let threshold = if engine.is_leader_for_view(msg_view) {
                        engine.leader_super_majority_threshold()
                    } else {
                        engine.super_majority_threshold()
                    };

                    if let Some(new_views) =
                        self.new_view_message.tally_by(msg_view, msg, threshold)
                    {
                        events.push(Event::NewView {
                            new_views: new_views.into_iter().map(|v| v.vote).collect(),
                            timeout_qc,
                        })
                    }
                }
            }
        }

        events
    }

    fn try_handle_leader<O: Overlay>(
        &mut self,
        engine: &Carnot<O>,
        events: &mut Vec<Event<CarnotTx>>,
    ) {
        if !self.proposal_seen.contains(&engine.current_view())
            && engine.is_leader_for_view(engine.current_view())
        {
            events.push(Event::Proposal {
                block: Block::new(
                    engine.current_view(),
                    Qc::Standard(engine.high_qc()),
                    [].into_iter(),
                ),
                stream: Box::pin(futures::stream::empty()),
            });
            self.proposal_seen.insert(engine.current_view());
        }
    }
}

struct Tally<T: core::hash::Hash + Eq> {
    cache: HashMap<View, HashSet<T>>,
    threshold: usize,
}

impl<T: core::hash::Hash + Eq> Default for Tally<T> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<T: core::hash::Hash + Eq> Tally<T> {
    fn new(threshold: usize) -> Self {
        Self {
            cache: Default::default(),
            threshold,
        }
    }

    fn tally(&mut self, view: View, message: T) -> Option<HashSet<T>> {
        self.tally_by(view, message, self.threshold)
    }

    fn tally_by(&mut self, view: View, message: T, threshold: usize) -> Option<HashSet<T>> {
        let entries = self
            .cache
            .entry(view)
            .and_modify(|entry| {
                entry.insert(message);
            })
            .or_default()
            .len();

        if entries >= threshold {
            Some(self.cache.remove(&view).unwrap())
        } else {
            None
        }
    }
}

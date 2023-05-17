use crate::node::carnot::messages::CarnotMessage;
use consensus_engine::{View, Qc, AggregateQc};
use nomos_consensus::network::messages::{NewViewMsg, TimeoutMsg, VoteMsg};
use nomos_consensus::Event::TimeoutQc;
use nomos_consensus::{Event, NodeId};
use nomos_core::block::{Block, BlockId};
use polars::chunked_array::object::hashbrown::HashSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type CarnotTx = [u8; 32];

#[derive(Default, Copy, Clone, Serialize, Deserialize)]
pub struct EventBuilderSettings {
    pub votes_threshold: usize,
    pub timeout_threshold: usize,
}

pub struct EventBuilder {
    blocks: HashMap<BlockId, Block<CarnotTx>>,
    vote_message: Tally<VoteMsg>,
    timeout_message: Tally<TimeoutMsg>,
    new_view_message: Tally<NewViewMsg>,
    config: EventBuilderSettings,
}

impl EventBuilder {
    pub fn new() -> Self {
        Self {
            vote_message: HashMap::new(),
            timeout_message: HashMap::new(),
            config: Default::default(),
            blocks: Default::default(),
            new_view_message: Default::default(),
        }
    }

    pub fn step(&mut self, messages: Vec<CarnotMessage>) -> Vec<Event<CarnotTx>> {
        let mut events = Vec::new();
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
                    let qc = msg.qc.clone().expect("empty QC from vote message")?;
                    if let Some(votes) = self.vote_message.tally(msg_view, msg) {
                        events.push(Event::Approve {
                            qc,
                            block: self
                                .blocks
                                .get(&block_id)
                                .expect(format!("cannot find block id {:?}", block_id).as_str())?,
                            votes: votes.into_iter().collect(),
                        })
                    }
                }
                CarnotMessage::Timeout(msg) => {
                    let msg_view = msg.vote.view;
                    if let Some(timeouts) = self.timeout_message.tally(msg_view, msg) {
                        events.push(Event::RootTimeout { timeouts })
                    }
                }
                CarnotMessage::NewView(msg) => {
                    let msg_view = msg.vote.view;
                    let timeout_qc = msg.vote.timeout_qc.clone();
                    if let Some(new_views) = self.new_view_message.tally(msg_view, msg) {
                        events.push(Event::ProposeBlock { qc: Qc::Aggregated(AggregateQc {
                            high_qc: timeout_qc.high_qc,
                            view: timeout_qc.view + 2,
                        }) })
                    }
                }
            }
        }

        events
    }
}

struct Tally<T> {
    cache: HashMap<View, Vec<T>>,
    threshold: usize,
}

impl<T> Default for Tally<T> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<T> Tally<T> {
    fn new(threshold: usize) -> Self {
        Self {
            cache: Default::default(),
            threshold,
        }
    }

    fn tally(&mut self, view: View, message: T) -> Option<Vec<T>> {
        let entries = self
            .cache
            .entry(view)
            .and_modify(|entry| entry.push(message))
            .or_default()
            .len();

        if entries >= self.threshold {
            Some(self.cache.remove(&view).unwrap())
        } else {
            None
        }
    }
}

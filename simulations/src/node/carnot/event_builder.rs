use crate::node::carnot::messages::CarnotMessage;
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
    vote_message: HashMap<View, Vec<VoteMsg>>,
    timeout_message: HashMap<View, Vec<TimeoutMsg>>,
    new_view_message: HashMap<View, Vec<NewViewMsg>>,
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
                    let entries = self
                        .vote_message
                        .entry(msg_view)
                        .and_modify(|entry| entry.push(msg))
                        .or_default()
                        .len();

                    if entries >= self.config.votes_threshold {
                        let entry = self.vote_message.remove(&msg_view).unwrap();
                        events.push(Event::Approve {
                            qc,
                            block: self
                                .blocks
                                .get(&block_id)
                                .expect(format!("cannot find block id {:?}", block_id).as_str())?,
                            votes: entry.into_iter().collect(),
                        })
                    }
                }
                CarnotMessage::Timeout(msg) => {
                    let msg_view = msg.vote.view;
                    let entires = self
                        .timeout_message
                        .entry(msg.vote.view)
                        .and_modify(|entry| entry.push(msg))
                        .or_default()
                        .len();

                    if entires >= self.config.timeout_threshold {
                        let entry = self.timeout_message.remove(&msg_view).unwrap();
                        events.push(Event::RootTimeout {
                            timeouts: entry.into_iter().map(|msg| msg.vote).collect(),
                        })
                    }
                }
                CarnotMessage::NewView(msg) => {
                    let msg_view = msg.vote.view;
                    let timeout_qc = msg.vote.timeout_qc.clone();
                    let entries = self
                        .new_view_message
                        .entry(msg.view)
                        .and_modify(|entry| entry.push(msg))
                        .or_default()
                        .len();

                    if entries >= self.config.votes_threshold {
                        let entry = self.new_view_message.remove(&msg_view).unwrap();
                        events.push(Event::NewView {
                            timeout_qc,
                            new_views: entry.into_iter().map(|msg| msg.vote).collect(),
                        })
                    }
                }
            }
        }

        events
    }
}

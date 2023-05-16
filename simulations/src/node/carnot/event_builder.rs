use crate::node::carnot::messages::CarnotMessage;
use nomos_consensus::network::messages::VoteMsg;
use nomos_consensus::Event;
use nomos_consensus::Event::TimeoutQc;
use nomos_core::block::Block;
use std::collections::HashMap;

pub type CarnotTx = [u8; 32];

#[derive(Default)]
pub struct EventBuilderConfiguration {
    pub votes_threshold: usize,
}

pub struct EventBuilder {
    vote_message: HashMap<View, Vec<VoteMsg>>,
    config: EventBuilderConfiguration,
}

impl EventBuilder {
    pub fn new() -> Self {
        Self {
            vote_message: HashMap::new(),
            config: Default::default(),
        }
    }

    pub fn step(&mut self, messages: &[CarnotMessage]) -> Vec<Event<CarnotTx>> {
        let mut events = Vec::new();
        for message in messages {
            match message {
                CarnotMessage::Proposal(msg) => {
                    events.push(Event::Proposal {
                        block: Block::from_bytes(&msg.chunk),
                        stream: Box::pin(futures::stream::empty()),
                    });
                }
                CarnotMessage::TimeoutQc(msg) => {
                    events.push(Event::TimeoutQc {
                        timeout_qc: msg.qc.clone(),
                    });
                }
                CarnotMessage::Vote(msg) => {
                    let msg_view = msg.vote.view;
                    let entry = self.vote_message.entry(msg_view).or_default();
                    entry.push(msg.clone());
                    if entry.len() >= self.config.votes_threshold {
                        events.push(Event::Approve {
                            qc: msg.qc.cloned().unwrap(),
                            block: msg.vote.block,
                            votes: entry.iter().cloned().collect(),
                        })
                    }
                    self.vote_message.remove(&msg_view);
                }
                CarnotMessage::Timeout(_) => {}
                CarnotMessage::NewView(_) => {}
            }
        }

        events
    }
}

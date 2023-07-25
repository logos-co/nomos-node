use consensus_engine::View;
use nomos_consensus::network::messages::{
    NewViewMsg, ProposalChunkMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg,
};

use crate::network::PayloadSize;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum CarnotMessage {
    Proposal(ProposalChunkMsg),
    Vote(VoteMsg),
    TimeoutQc(TimeoutQcMsg),
    Timeout(TimeoutMsg),
    NewView(NewViewMsg),
}

impl CarnotMessage {
    pub fn view(&self) -> View {
        match self {
            CarnotMessage::Proposal(msg) => msg.view,
            CarnotMessage::Vote(msg) => msg.vote.view,
            CarnotMessage::TimeoutQc(msg) => msg.qc.view(),
            CarnotMessage::Timeout(msg) => msg.vote.view,
            CarnotMessage::NewView(msg) => msg.vote.view,
        }
    }
}

impl PayloadSize for CarnotMessage {
    fn size_bytes(&self) -> u32 {
        match self {
            CarnotMessage::Proposal(p) => 56 + p.chunk.len() as u32,
            CarnotMessage::Vote(_) => 128,
            CarnotMessage::TimeoutQc(_) => 112,
            CarnotMessage::Timeout(_) => 200,
            CarnotMessage::NewView(_) => 192,
        }
    }
}

use consensus_engine::View;
use nomos_consensus::network::messages::{
    NewViewMsg, ProposalMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg,
};

use crate::network::PayloadSize;

#[derive(Debug, Eq, PartialEq, Hash, Clone)]
pub enum CarnotMessage {
    Proposal(ProposalMsg),
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
            CarnotMessage::Proposal(p) => {
                (std::mem::size_of::<ProposalMsg>() + p.data.len()) as u32
            }
            CarnotMessage::Vote(_) => std::mem::size_of::<VoteMsg>() as u32,
            CarnotMessage::TimeoutQc(_) => std::mem::size_of::<TimeoutQcMsg>() as u32,
            CarnotMessage::Timeout(_) => std::mem::size_of::<TimeoutMsg>() as u32,
            CarnotMessage::NewView(_) => std::mem::size_of::<NewViewMsg>() as u32,
        }
    }
}

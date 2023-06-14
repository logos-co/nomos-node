use consensus_engine::View;
use nomos_consensus::network::messages::{
    NewViewMsg, ProposalChunkMsg, TimeoutMsg, TimeoutQcMsg, VoteMsg,
};

#[derive(Eq, PartialEq, Hash, Clone)]
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
            CarnotMessage::TimeoutQc(msg) => msg.qc.view,
            CarnotMessage::Timeout(msg) => msg.vote.view,
            CarnotMessage::NewView(msg) => msg.vote.view,
        }
    }
}

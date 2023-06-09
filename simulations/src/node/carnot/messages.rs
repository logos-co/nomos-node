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

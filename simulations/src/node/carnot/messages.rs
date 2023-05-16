use nomos_consensus::network::messages::{NewViewMsg, ProposalChunkMsg, TimeoutQcMsg, VoteMsg};

pub(crate) enum CarnotMessage {
    Proposal(ProposalChunkMsg),
    Vote(VoteMsg),
    TimeoutQc(TimeoutQcMsg),
    Timeout(TimeoutQcMsg),
    NewView(NewViewMsg),
}

use crate::{Approval, BlockChunk, NodeId};

pub struct ProposalChunkMsg {
    index: u8,
    chunk: BlockChunk,
}

// TODO: this is completely temporal and match no reality at all, but it will help use fake some of the process
impl ProposalChunkMsg {
    pub fn as_bytes<const SIZE: usize>(&self) -> [u8; SIZE] {
        [self.index; SIZE]
    }

    pub fn from_bytes<const SIZE: usize>(data: [u8; SIZE]) -> Self {
        Self {
            index: data[0],
            chunk: BlockChunk,
        }
    }
}

pub struct ApprovalMsg {
    source: NodeId,
    approval: Approval,
}

impl ApprovalMsg {
    pub fn as_bytes(&self) -> Box<[u8]> {
        self.source.into()
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            source: NodeId::try_from(data).unwrap(),
            approval: Approval,
        }
    }
}

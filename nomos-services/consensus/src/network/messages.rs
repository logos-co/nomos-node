// std
// crates
use bytes::Bytes;
// internal
use crate::{Approval, NodeId};

pub struct ProposalChunkMsg {
    pub chunk: Bytes,
}

impl ProposalChunkMsg {
    pub fn as_bytes(&self) -> &[u8] {
        &self.chunk
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        Self {
            chunk: Bytes::from(data.to_vec()),
        }
    }
}

pub struct ApprovalMsg {
    pub source: NodeId,
    pub approval: Approval,
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

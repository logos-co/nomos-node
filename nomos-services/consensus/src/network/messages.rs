// std
// crates
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal
use crate::NodeId;
use nomos_core::wire;

#[derive(Clone)]
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

#[derive(Serialize, Deserialize)]
pub struct ApprovalMsg<Vote> {
    pub source: NodeId,
    pub approval: Vote,
}

impl<Vote> ApprovalMsg<Vote>
where
    Vote: Serialize,
{
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }
}

impl<Vote> ApprovalMsg<Vote>
where
    Vote: DeserializeOwned,
{
    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}

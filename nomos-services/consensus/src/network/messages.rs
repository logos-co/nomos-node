// std
// crates
use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal
use crate::NodeId;
use consensus_engine::{TimeoutQc, View};
use nomos_core::wire;

#[derive(Clone, Serialize, Deserialize)]
pub struct ProposalChunkMsg {
    pub chunk: Bytes,
    pub view: View,
}

impl ProposalChunkMsg {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
pub struct VoteMsg<Vote> {
    pub source: NodeId,
    pub vote: Vote,
}

impl<Vote> VoteMsg<Vote>
where
    Vote: Serialize,
{
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }
}

impl<Vote> VoteMsg<Vote>
where
    Vote: DeserializeOwned,
{
    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}

#[derive(Serialize, Deserialize)]
pub struct TimeoutQcMsg {
    pub source: NodeId,
    pub qc: TimeoutQc,
}

impl TimeoutQcMsg {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }

    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}

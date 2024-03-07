// std
// crates
use serde::{Deserialize, Serialize};
// internal
use nomos_core::wire;

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq, Hash)]
pub struct BlockMsg {
    pub data: Box<[u8]>,
}

#[derive(Serialize, Deserialize)]
pub enum NetworkMessage {
    Block(BlockMsg),
}

impl NetworkMessage {
    pub fn as_bytes(&self) -> Box<[u8]> {
        wire::serialize(self).unwrap().into_boxed_slice()
    }
    pub fn from_bytes(data: &[u8]) -> Self {
        wire::deserialize(data).unwrap()
    }
}

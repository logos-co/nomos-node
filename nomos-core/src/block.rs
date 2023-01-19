// std
// crates
use bytes::Bytes;
// internal

/// A block
#[derive(Clone, Debug)]
pub struct Block;

/// A block header
#[derive(Clone, Debug)]
pub struct BlockHeader;

/// Identifier of a block
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct BlockId;

impl Block {
    /// Encode block into bytes
    pub fn as_bytes(&self) -> Bytes {
        Bytes::new()
    }

    pub fn from_bytes(_: Bytes) -> Self {
        Self
    }
}

impl BlockHeader {
    pub fn id(&self) -> BlockId {
        BlockId
    }
}

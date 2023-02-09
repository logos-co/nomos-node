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
pub type BlockId = [u8; 32];

impl Block {
    /// Encode block into bytes
    pub fn as_bytes(&self) -> Bytes {
        Bytes::new()
    }

    pub fn from_bytes(_: Bytes) -> Self {
        Self
    }

    pub fn header(&self) -> BlockHeader {
        BlockHeader
    }
}

impl BlockHeader {
    pub fn id(&self) -> BlockId {
        todo!()
    }
}

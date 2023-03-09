// std
// crates
use bytes::Bytes;
use serde::{Deserialize, Serialize};
// internal

/// A block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<Tx> {
    header: BlockHeader,
    transactions: Vec<Tx>,
}

/// A block header
#[derive(Copy, Clone, Default, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    id: BlockId,
}

/// Identifier of a block
pub type BlockId = [u8; 32];

impl<Tx: serde::de::DeserializeOwned> Block<Tx> {
    pub fn new(header: BlockHeader, txs: impl Iterator<Item = Tx>) -> Self {
        Self {
            header,
            transactions: txs.collect(),
        }
    }

    /// Encode block into bytes
    pub fn as_bytes(&self) -> Bytes {
        Bytes::new()
    }

    pub fn header(&self) -> BlockHeader {
        self.header
    }

    pub fn transactions(&self) -> &[Tx] {
        &self.transactions
    }
}

impl BlockHeader {
    pub fn id(&self) -> BlockId {
        self.id
    }
}

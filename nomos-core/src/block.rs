use indexmap::IndexSet;
// std
use core::hash::Hash;
// crates
use bytes::Bytes;
use serde::{Deserialize, Serialize};
// internal

pub type TxHash = [u8; 32];

/// A block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<TxId: Eq + Hash> {
    header: BlockHeader,
    transactions: IndexSet<TxId>,
}

/// A block header
#[derive(Copy, Clone, Default, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    id: BlockId,
}

/// Identifier of a block
pub type BlockId = [u8; 32];

impl<TxId: Eq + Hash> Block<TxId> {
    pub fn new(header: BlockHeader, txs: impl Iterator<Item = TxId>) -> Self {
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

    pub fn transactions(&self) -> impl Iterator<Item = &TxId> + '_ {
        self.transactions.iter()
    }
}

impl BlockHeader {
    pub fn id(&self) -> BlockId {
        self.id
    }
}

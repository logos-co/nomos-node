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
pub struct Block<Qc: Clone, TxId: Clone + Eq + Hash> {
    header: BlockHeader<Qc>,
    transactions: IndexSet<TxId>,
}

/// A block header
#[derive(Copy, Clone, Default, Debug, Serialize, Deserialize)]
pub struct BlockHeader<Qc: Clone> {
    id: BlockId,
    qc: Qc,
}

/// Identifier of a block
pub type BlockId = [u8; 32];

impl<Qc: Clone, TxId: Clone + Eq + Hash> Block<Qc, TxId> {
    pub fn new(qc: Qc, txs: impl Iterator<Item = TxId>) -> Self {
        let transactions = txs.collect();
        // FIXME: Calculate header Id
        let header = BlockHeader { id: [0; 32], qc };
        Self {
            header,
            transactions,
        }
    }

    /// Encode block into bytes
    pub fn as_bytes(&self) -> Bytes {
        Bytes::new()
    }

    pub fn header(&self) -> BlockHeader<Qc> {
        self.header.clone()
    }

    pub fn transactions(&self) -> impl Iterator<Item = &TxId> + '_ {
        self.transactions.iter()
    }
}

impl<Qc: Clone> BlockHeader<Qc> {
    pub fn id(&self) -> BlockId {
        self.id
    }

    pub fn qc(&self) -> &Qc {
        &self.qc
    }
}

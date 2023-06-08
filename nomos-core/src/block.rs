use indexmap::IndexSet;
// std
use core::hash::Hash;
// crates
use crate::wire;
use bytes::Bytes;
use consensus_engine::{Qc, View};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal

pub type TxHash = [u8; 32];

/// A block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<TxId: Clone + Eq + Hash> {
    header: consensus_engine::Block,
    transactions: IndexSet<TxId>,
}

/// Identifier of a block
pub type BlockId = [u8; 32];

impl<TxId: Clone + Eq + Hash + Serialize + DeserializeOwned> Block<TxId> {
    pub fn new(view: View, parent_qc: Qc, txs: impl Iterator<Item = TxId>) -> Self {
        let transactions = txs.collect();
        let header = consensus_engine::Block {
            id: [0; 32],
            view,
            parent_qc,
        };

        let mut s = Self {
            header,
            transactions,
        };
        let id = block_id_from_wire_content(&s);
        s.header.id = id;
        s
    }

    pub fn header(&self) -> &consensus_engine::Block {
        &self.header
    }

    pub fn transactions(&self) -> impl Iterator<Item = &TxId> + '_ {
        self.transactions.iter()
    }
}

pub fn block_id_from_wire_content<Tx: Clone + Eq + Hash + Serialize + DeserializeOwned>(
    block: &Block<Tx>,
) -> consensus_engine::BlockId {
    use blake2::digest::{consts::U32, Digest};
    use blake2::Blake2b;
    let bytes = block.as_bytes();
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

impl<TxId: Clone + Eq + Hash + Serialize + DeserializeOwned> Block<TxId> {
    /// Encode block into bytes
    pub fn as_bytes(&self) -> Bytes {
        wire::serialize(self).unwrap().into()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut result: Self = wire::deserialize(bytes).unwrap();
        result.header.id = block_id_from_wire_content(&result);
        result
    }
}

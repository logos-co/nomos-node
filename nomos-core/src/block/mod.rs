pub mod builder;

use consensus_engine::overlay::RandomBeaconState;
use indexmap::IndexSet;
// std
use core::hash::Hash;
// crates
use crate::wire;
use bytes::Bytes;
pub use consensus_engine::BlockId;
use consensus_engine::{LeaderProof, NodeId, Qc, View};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal

pub type TxHash = [u8; 32];

/// A block
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> {
    header: consensus_engine::Block,
    beacon: RandomBeaconState,
    cl_transactions: IndexSet<Tx>,
    bl_blobs: IndexSet<BlobCertificate>,
}

impl<
        Tx: Clone + Eq + Hash + Serialize + DeserializeOwned,
        BlobCertificate: Clone + Eq + Hash + Serialize + DeserializeOwned,
    > Block<Tx, BlobCertificate>
{
    pub fn new(
        view: View,
        parent_qc: Qc,
        txs: impl Iterator<Item = Tx>,
        blobs: impl Iterator<Item = BlobCertificate>,
        proposer: NodeId,
        beacon: RandomBeaconState,
    ) -> Self {
        let transactions = txs.collect();
        let blobs = blobs.collect();
        let header = consensus_engine::Block {
            id: BlockId::zeros(),
            view,
            parent_qc,
            leader_proof: LeaderProof::LeaderId {
                leader_id: proposer,
            },
        };
        let mut s = Self {
            header,
            beacon,
            cl_transactions: transactions,
            bl_blobs: blobs,
        };
        let id = block_id_from_wire_content(&s);
        s.header.id = id;
        s
    }
}

impl<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> Block<Tx, BlobCertificate> {
    pub fn header(&self) -> &consensus_engine::Block {
        &self.header
    }

    pub fn transactions(&self) -> impl Iterator<Item = &Tx> + '_ {
        self.cl_transactions.iter()
    }

    pub fn blobs(&self) -> impl Iterator<Item = &BlobCertificate> + '_ {
        self.bl_blobs.iter()
    }

    pub fn beacon(&self) -> &RandomBeaconState {
        &self.beacon
    }
}

pub fn block_id_from_wire_content<
    Tx: Clone + Eq + Hash + Serialize + DeserializeOwned,
    BlobCertificate: Clone + Eq + Hash + Serialize + DeserializeOwned,
>(
    block: &Block<Tx, BlobCertificate>,
) -> consensus_engine::BlockId {
    use blake2::digest::{consts::U32, Digest};
    use blake2::Blake2b;
    let bytes = block.as_bytes();
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(bytes);
    BlockId::new(hasher.finalize().into())
}

impl<
        Tx: Clone + Eq + Hash + Serialize + DeserializeOwned,
        BlobCertificate: Clone + Eq + Hash + Serialize + DeserializeOwned,
    > Block<Tx, BlobCertificate>
{
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

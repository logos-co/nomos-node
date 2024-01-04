pub mod builder;

use carnot_engine::overlay::RandomBeaconState;
use indexmap::IndexSet;
// std
use core::hash::Hash;
// crates
use crate::wire;
use ::serde::{
    de::{DeserializeOwned, Deserializer},
    Deserialize, Serialize, Serializer,
};
use bytes::Bytes;
pub use carnot_engine::BlockId;
use carnot_engine::{LeaderProof, NodeId, Qc, View};
// internal

pub type TxHash = [u8; 32];

/// A block
#[derive(Clone, Debug)]
pub struct Block<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> {
    header: carnot_engine::Block,
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
        let header = carnot_engine::Block {
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
    pub fn header(&self) -> &carnot_engine::Block {
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
) -> carnot_engine::BlockId {
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

mod serde {
    use super::*;
    // use ::serde::{de::Deserializer, Deserialize, Serialize};

    /// consensus_engine::Block but without the id field, which will be computed
    /// from the rest of the block.
    #[derive(Serialize, Deserialize)]
    struct StrippedHeader {
        pub view: View,
        pub parent_qc: Qc,
        pub leader_proof: LeaderProof,
    }

    #[derive(Serialize, Deserialize)]
    struct StrippedBlock<Tx: Clone + Eq + Hash, BlobCertificate: Clone + Eq + Hash> {
        header: StrippedHeader,
        beacon: RandomBeaconState,
        cl_transactions: IndexSet<Tx>,
        bl_blobs: IndexSet<BlobCertificate>,
    }

    impl<
            'de,
            Tx: Clone + Eq + Hash + Serialize + DeserializeOwned,
            BlobCertificate: Clone + Eq + Hash + Serialize + DeserializeOwned,
        > Deserialize<'de> for Block<Tx, BlobCertificate>
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let StrippedBlock {
                header,
                beacon,
                cl_transactions,
                bl_blobs,
            } = StrippedBlock::deserialize(deserializer)?;
            let header = carnot_engine::Block {
                id: BlockId::zeros(),
                view: header.view,
                parent_qc: header.parent_qc,
                leader_proof: header.leader_proof,
            };
            let mut block = Block {
                beacon,
                cl_transactions,
                bl_blobs,
                header,
            };
            block.header.id = block_id_from_wire_content(&block);
            Ok(block)
        }
    }

    impl<
            Tx: Clone + Eq + Hash + Serialize + DeserializeOwned,
            BlobCertificate: Clone + Eq + Hash + Serialize + DeserializeOwned,
        > Serialize for Block<Tx, BlobCertificate>
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            // TODO: zero copy serialization
            let block = StrippedBlock {
                header: StrippedHeader {
                    view: self.header.view,
                    parent_qc: self.header.parent_qc.clone(),
                    leader_proof: self.header.leader_proof.clone(),
                },
                beacon: self.beacon.clone(),
                cl_transactions: self.cl_transactions.clone(),
                bl_blobs: self.bl_blobs.clone(),
            };
            block.serialize(serializer)
        }
    }
}

use super::{ContentId, HeaderId};
use crate::crypto::Blake2b;
use blake2::Digest;
use cryptarchia_engine::Slot;
use cryptarchia_ledger::leader_proof::{LeaderProof, Risc0LeaderProof};
use serde::{Deserialize, Serialize};

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub struct Nonce([u8; 32]);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    parent: HeaderId,
    slot: Slot,
    // TODO: move this to common header fields
    // length of block contents in bytes
    content_size: u32,
    // id of block contents
    content_id: ContentId,
    leader_proof: Risc0LeaderProof,
    orphaned_leader_proofs: Vec<Header>,
}

impl Header {
    pub fn parent(&self) -> HeaderId {
        self.parent
    }

    fn update_hasher(&self, h: &mut Blake2b) {
        h.update(b"\x01");
        h.update(self.content_size.to_be_bytes());
        h.update(self.content_id.0);
        h.update(self.slot.to_be_bytes());
        h.update(self.parent.0);

        h.update(self.leader_proof.nullifier().as_bytes());
        h.update(self.leader_proof.evolved_commitment().as_bytes());

        for proof in &self.orphaned_leader_proofs {
            proof.update_hasher(h)
        }
    }

    pub fn id(&self) -> HeaderId {
        let mut h = Blake2b::new();
        self.update_hasher(&mut h);
        HeaderId(h.finalize().into())
    }

    pub fn leader_proof(&self) -> &impl LeaderProof {
        &self.leader_proof
    }

    pub fn slot(&self) -> Slot {
        self.slot
    }

    pub fn orphaned_proofs(&self) -> &[Header] {
        &self.orphaned_leader_proofs
    }

    pub fn new(
        parent: HeaderId,
        content_size: u32,
        content_id: ContentId,
        slot: Slot,
        leader_proof: Risc0LeaderProof,
    ) -> Self {
        Self {
            parent,
            content_size,
            content_id,
            slot,
            leader_proof,
            orphaned_leader_proofs: vec![],
        }
    }

    pub fn with_orphaned_proofs(mut self, orphaned_leader_proofs: Vec<Header>) -> Self {
        self.orphaned_leader_proofs = orphaned_leader_proofs;
        self
    }
}

pub struct Builder {
    parent: HeaderId,
    slot: Slot,
    leader_proof: Risc0LeaderProof,
    orphaned_leader_proofs: Vec<Header>,
}

impl Builder {
    pub fn new(parent: HeaderId, slot: Slot, leader_proof: Risc0LeaderProof) -> Self {
        Self {
            parent,
            slot,
            leader_proof,
            orphaned_leader_proofs: vec![],
        }
    }

    pub fn with_orphaned_proofs(mut self, orphaned_leader_proofs: Vec<Header>) -> Self {
        self.orphaned_leader_proofs = orphaned_leader_proofs;
        self
    }

    pub fn build(self, content_id: ContentId, content_size: u32) -> Header {
        Header {
            parent: self.parent,
            slot: self.slot,
            content_size,
            content_id,
            leader_proof: self.leader_proof,
            orphaned_leader_proofs: self.orphaned_leader_proofs,
        }
    }
}

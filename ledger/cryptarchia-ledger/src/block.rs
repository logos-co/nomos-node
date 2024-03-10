use crate::{crypto::Blake2b, leader_proof::LeaderProof};
use blake2::Digest;
use cryptarchia_engine::Slot;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct HeaderId([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct ContentId([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub struct Nonce([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Header {
    parent: HeaderId,
    // length of block contents in bytes
    content_size: u32,
    // id of block contents
    content_id: ContentId,
    slot: Slot,
    leader_proof: LeaderProof,
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

        h.update(self.leader_proof.commitment());
        h.update(self.leader_proof.nullifier());
        h.update(self.leader_proof.evolved_commitment());

        for proof in &self.orphaned_leader_proofs {
            proof.update_hasher(h)
        }
    }

    pub fn id(&self) -> HeaderId {
        let mut h = Blake2b::new();
        self.update_hasher(&mut h);
        HeaderId(h.finalize().into())
    }

    pub fn leader_proof(&self) -> &LeaderProof {
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
        leader_proof: LeaderProof,
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

// ----------- conversions

impl From<[u8; 32]> for Nonce {
    fn from(nonce: [u8; 32]) -> Self {
        Self(nonce)
    }
}

impl From<Nonce> for [u8; 32] {
    fn from(nonce: Nonce) -> [u8; 32] {
        nonce.0
    }
}

impl From<[u8; 32]> for HeaderId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<HeaderId> for [u8; 32] {
    fn from(id: HeaderId) -> Self {
        id.0
    }
}

impl From<[u8; 32]> for ContentId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<ContentId> for [u8; 32] {
    fn from(id: ContentId) -> Self {
        id.0
    }
}

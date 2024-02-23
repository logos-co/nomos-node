use crate::{crypto::Blake2b, time::Slot};
use blake2::Digest;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct HeaderId([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct ContentId([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub struct Nonce([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct LeaderProof {
    commitment: Commitment,
    nullifier: Nullifier,
    slot: Slot,
    evolved_commitment: Commitment,
}

impl LeaderProof {
    pub fn commitment(&self) -> &Commitment {
        &self.commitment
    }

    pub fn nullifier(&self) -> &Nullifier {
        &self.nullifier
    }

    pub fn slot(&self) -> Slot {
        self.slot
    }

    pub fn evolved_commitment(&self) -> &Commitment {
        &self.evolved_commitment
    }

    #[cfg(test)]
    pub fn dummy(slot: Slot) -> Self {
        Self {
            commitment: Commitment([0; 32]),
            nullifier: Nullifier([0; 32]),
            slot,
            evolved_commitment: Commitment([0; 32]),
        }
    }

    pub fn new(
        commitment: Commitment,
        nullifier: Nullifier,
        slot: Slot,
        evolved_commitment: Commitment,
    ) -> Self {
        Self {
            commitment,
            nullifier,
            slot,
            evolved_commitment,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct Commitment([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct Nullifier([u8; 32]);

impl Nullifier {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

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

    pub fn id(&self) -> HeaderId {
        let mut h = Blake2b::new();
        h.update(b"\x01");
        h.update(self.content_size.to_be_bytes());
        h.update(self.content_id.0);
        h.update(self.slot.to_be_bytes());
        h.update(self.parent.0);

        // TODO: add proof
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    header: Header,
    _contents: (),
}

impl Block {
    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn new(header: Header) -> Self {
        Self {
            header,
            _contents: (),
        }
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

impl From<[u8; 32]> for Commitment {
    fn from(commitment: [u8; 32]) -> Self {
        Self(commitment)
    }
}

impl From<Commitment> for [u8; 32] {
    fn from(commitment: Commitment) -> Self {
        commitment.0
    }
}

impl From<[u8; 32]> for Nullifier {
    fn from(nullifier: [u8; 32]) -> Self {
        Self(nullifier)
    }
}

impl From<Nullifier> for [u8; 32] {
    fn from(nullifier: Nullifier) -> Self {
        nullifier.0
    }
}

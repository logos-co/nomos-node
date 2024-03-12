use cryptarchia_engine::Slot;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

impl AsRef<[u8]> for Nullifier {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for Commitment {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// ----------- serialization
use crate::utils::serialize_bytes_newtype;

serialize_bytes_newtype!(Commitment);
serialize_bytes_newtype!(Nullifier);

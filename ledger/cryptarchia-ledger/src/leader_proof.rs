use leader_proof_statements::LeaderPublic;

pub trait LeaderProof {
    fn verify(&self, public_inputs: &LeaderPublic) -> bool;
    // The nullifier of the coin used in the proof
    fn nullifier(&self) -> cl::Nullifier;
    // The evolved commitment for the coin used in the proof
    fn evolved_commitment(&self) -> cl::NoteCommitment;
    // The merkle root used for the proof
    // This is needed here because the proof could be using an old merkle root, and we don't want a verifying
    // node to have to guess which one it's using.
    fn merke_root(&self) -> [u8; 32];

    fn to_orphan_proof(&self) -> OrphanProof {
        OrphanProof {
            nullifier: self.nullifier(),
            commitment: self.evolved_commitment(),
            cm_root: self.merke_root(),
        }
    }
}

pub struct OrphanProof {
    pub(super) nullifier: cl::Nullifier,
    pub(super) commitment: cl::NoteCommitment,
    pub(super) cm_root: [u8; 32],
}

#[cfg(feature = "risc0_proof")]
mod risc0 {
    use super::*;
    use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Debug, Clone)]
    pub struct Risc0LeaderProof {
        public_inputs: LeaderPublic,
        risc0_receipt: risc0_zkvm::Receipt,
    }

    #[cfg(feature = "prove")]
    impl Risc0LeaderProof {
        pub fn build(
            public_inputs: LeaderPublic,
            private_inputs: leader_proof_statements::LeaderPrivate,
        ) -> Result<Self, nomos_pol_prover::Error> {
            let risc0_receipt = nomos_pol_prover::prove(public_inputs, private_inputs)?;
            Ok(Self {
                public_inputs,
                risc0_receipt,
            })
        }
    }

    impl LeaderProof for Risc0LeaderProof {
        fn verify(&self, public_inputs: &LeaderPublic) -> bool {
            public_inputs == &self.public_inputs
                && self
                    .risc0_receipt
                    .verify(nomos_pol_risc0_proofs::PROOF_OF_LEADERSHIP_ID)
                    .is_ok()
        }

        fn nullifier(&self) -> cl::Nullifier {
            self.public_inputs.nullifier
        }

        fn evolved_commitment(&self) -> cl::NoteCommitment {
            self.public_inputs.evolved_commitment
        }

        fn merke_root(&self) -> [u8; 32] {
            self.public_inputs.cm_root
        }
    }

    impl Serialize for Risc0LeaderProof {
        fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            self.risc0_receipt.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for Risc0LeaderProof {
        fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let risc0_receipt = risc0_zkvm::Receipt::deserialize(deserializer)?;
            let public_inputs = risc0_receipt
                .journal
                .decode()
                .map_err(|e| D::Error::custom(format!("Invalid public inputs: {e}")))?;

            Ok(Self {
                public_inputs,
                risc0_receipt,
            })
        }
    }
}

#[cfg(feature = "risc0_proof")]
pub use risc0::Risc0LeaderProof;

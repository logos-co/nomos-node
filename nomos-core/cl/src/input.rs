/// This module defines the partial transaction structure.
///
/// Partial transactions, as the name suggests, are transactions
/// which on their own may not balance (i.e. \sum inputs != \sum outputs)
use crate::{
    merkle,
    note::{Covenant, NoteWitness},
    nullifier::{Nullifier, NullifierSecret},
    Nonce,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    pub nullifier: Nullifier,
    pub covenant: Covenant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputWitness {
    pub note: NoteWitness,
    pub nf_sk: NullifierSecret,
    pub cm_path: Vec<crate::merkle::PathNode>,
}

impl InputWitness {
    pub fn new(note: NoteWitness, nf_sk: NullifierSecret, cm_path: Vec<merkle::PathNode>) -> Self {
        Self {
            note,
            nf_sk,
            cm_path,
        }
    }

    pub fn from_output(
        output: crate::OutputWitness,
        nf_sk: NullifierSecret,
        cm_path: Vec<merkle::PathNode>,
    ) -> Self {
        assert_eq!(nf_sk.commit(), output.nf_pk);
        Self::new(output.note, nf_sk, cm_path)
    }

    pub fn public(output: crate::OutputWitness, cm_path: Vec<merkle::PathNode>) -> Self {
        let nf_sk = NullifierSecret::zero();
        assert_eq!(nf_sk.commit(), output.nf_pk); // ensure the output was a public UTXO
        Self::new(output.note, nf_sk, cm_path)
    }

    pub fn evolved_nonce(&self, domain: &[u8]) -> Nonce {
        self.note.evolved_nonce(self.nf_sk, domain)
    }

    pub fn evolve_output(&self, domain: &[u8]) -> crate::OutputWitness {
        crate::OutputWitness {
            note: NoteWitness {
                nonce: self.evolved_nonce(domain),
                ..self.note
            },
            nf_pk: self.nf_sk.commit(),
        }
    }

    pub fn nullifier(&self) -> Nullifier {
        Nullifier::new(self.nf_sk, self.note_commitment())
    }

    pub fn commit(&self) -> Input {
        Input {
            nullifier: self.nullifier(),
            covenant: self.note.covenant,
        }
    }

    pub fn note_commitment(&self) -> crate::NoteCommitment {
        self.note.commit(self.nf_sk.commit())
    }
}

impl Input {
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes = [0u8; 64];
        bytes[..32].copy_from_slice(self.nullifier.as_bytes());
        bytes[32..64].copy_from_slice(&self.covenant.0);
        bytes
    }
}

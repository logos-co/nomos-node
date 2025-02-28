use serde::{Deserialize, Serialize};

use crate::{
    note::{NoteCommitment, NoteWitness},
    nullifier::NullifierCommitment,
    NullifierSecret,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Output {
    pub note_comm: NoteCommitment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputWitness {
    pub note: NoteWitness,
    pub nf_pk: NullifierCommitment,
}

impl OutputWitness {
    #[must_use] pub const fn new(note: NoteWitness, nf_pk: NullifierCommitment) -> Self {
        Self { note, nf_pk }
    }

    #[must_use] pub fn public(note: NoteWitness) -> Self {
        let nf_pk = NullifierSecret::zero().commit();
        Self { note, nf_pk }
    }

    #[must_use] pub fn commit_note(&self) -> NoteCommitment {
        self.note.commit(self.nf_pk)
    }

    #[must_use] pub fn commit(&self) -> Output {
        Output {
            note_comm: self.commit_note(),
        }
    }
}

impl Output {
    #[must_use] pub const fn to_bytes(&self) -> [u8; 32] {
        self.note_comm.0
    }
}

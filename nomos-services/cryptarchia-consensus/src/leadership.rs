use cl::{input::InputWitness, note::NoteWitness, nullifier::Nullifier};
use cryptarchia_engine::Slot;
use cryptarchia_ledger::{leader_proof::Risc0LeaderProof, Config, EpochState, NoteTree};
use leader_proof_statements::{LeaderPrivate, LeaderPublic};
use nomos_core::header::HeaderId;
use std::collections::HashMap;

pub struct Leader {
    // for each block, the indexes in the note tree of the notes we control
    notes: HashMap<HeaderId, Vec<InputWitness>>,
    config: cryptarchia_ledger::Config,
}

impl Leader {
    pub fn new(genesis: HeaderId, notes: Vec<InputWitness>, config: Config) -> Self {
        Leader {
            notes: HashMap::from([(genesis, notes)]),
            config,
        }
    }

    // Signal that the chain extended with a new header, possibly evolving a leader notes in the process
    // FIXME: this implementation does not delete old coins and will attempt to re-use a note in different forks,
    //        we should use the orphan proofs mechanism to handle this.
    pub fn follow_chain(&mut self, parent_id: HeaderId, id: HeaderId, to_evolve: Nullifier) {
        if let Some(notes) = self.notes.get(&parent_id) {
            let notes = notes
                .iter()
                .map(|note| {
                    if note.nullifier() == to_evolve {
                        evolve(note)
                    } else {
                        *note
                    }
                })
                .collect();
            self.notes.insert(id, notes);
        }
    }

    pub async fn build_proof_for(
        &self,
        note_tree: &NoteTree,
        epoch_state: &EpochState,
        slot: Slot,
        parent: HeaderId,
    ) -> Option<Risc0LeaderProof> {
        let notes = self.notes.get(&parent)?;
        for note in notes {
            let Some(index) = note_tree
                .commitments()
                .iter()
                .position(|cm| cm == &note.note_commitment())
            else {
                continue;
            };

            let input_cm_path = note_tree
                .witness(index)
                .expect("Note was found in the tree");
            let public_inputs = LeaderPublic::new(
                note_tree.root(),
                *epoch_state.nonce(),
                slot.into(),
                self.config.consensus_config.active_slot_coeff,
                epoch_state.total_stake(),
                note.nullifier(),
                note.evolve_output(b"NOMOS_POL").commit_note(),
            );
            if public_inputs.check_winning(note) {
                tracing::debug!(
                    "leader for slot {:?}, {:?}/{:?}",
                    slot,
                    note.note.value,
                    epoch_state.total_stake()
                );
                let input = *note;
                return Some(
                    tokio::task::spawn_blocking(move || {
                        Risc0LeaderProof::build(
                            public_inputs,
                            LeaderPrivate {
                                input,
                                input_cm_path,
                            },
                        )
                    })
                    .await
                    .ok()?,
                );
            }
        }

        None
    }
}

fn evolve(note: &InputWitness) -> InputWitness {
    InputWitness {
        note: NoteWitness {
            nonce: note.evolved_nonce(b"NOMOS_POL"),
            ..note.note
        },
        nf_sk: note.nf_sk,
    }
}

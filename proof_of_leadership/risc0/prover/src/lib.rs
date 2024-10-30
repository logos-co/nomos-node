use leader_proof_statements::{LeaderPrivate, LeaderPublic};
use risc0_zkvm::Receipt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to produce proof")]
    ProofError(#[from] anyhow::Error),
}

pub fn prove(leader_public: LeaderPublic, leader_private: LeaderPrivate) -> Result<Receipt, Error> {
    let env = risc0_zkvm::ExecutorEnv::builder()
        .write(&leader_public)
        .unwrap()
        .write(&leader_private)
        .unwrap()
        .build()
        .unwrap();

    // Obtain the default prover.
    let prover = risc0_zkvm::default_prover();

    let start_t = std::time::Instant::now();

    // Proof information by proving the specified ELF binary.
    // This struct contains the receipt along with statistics about execution of the guest
    let opts = risc0_zkvm::ProverOpts::succinct();
    let prove_info =
        prover.prove_with_opts(env, nomos_pol_risc0_proofs::PROOF_OF_LEADERSHIP_ELF, &opts)?;

    tracing::debug!(
        "STARK prover time: {:.2?}, total_cycles: {}",
        start_t.elapsed(),
        prove_info.stats.total_cycles
    );
    // extract the receipt.
    Ok(prove_info.receipt)
}

#[cfg(test)]
mod test {
    use super::*;
    use cl::{note::NoteWitness, nullifier::NullifierSecret, InputWitness};
    use rand::thread_rng;

    const MAX_NOTE_COMMS: usize = 1 << 8;

    // derive-unit(NOMOS_NMO)
    pub const NMO_UNIT: [u8; 32] = [
        67, 50, 140, 228, 181, 204, 226, 242, 254, 193, 239, 51, 237, 68, 36, 126, 124, 227, 60,
        112, 223, 195, 146, 236, 5, 21, 42, 215, 48, 122, 25, 195,
    ];

    fn note_commitment_leaves(
        note_commitments: &[cl::NoteCommitment],
    ) -> [[u8; 32]; MAX_NOTE_COMMS] {
        let note_comm_bytes =
            Vec::from_iter(note_commitments.iter().map(|c| c.as_bytes().to_vec()));
        let cm_leaves = cl::merkle::padded_leaves::<MAX_NOTE_COMMS>(&note_comm_bytes);
        cm_leaves
    }

    #[test]
    fn test_leader_prover() {
        let mut rng = thread_rng();

        let nf_sk = NullifierSecret::random(&mut rng);
        let note = NoteWitness::basic(32, NMO_UNIT, &mut rng);

        let notes = vec![note.commit(nf_sk.commit())];
        let epoch_nonce = [0u8; 32];
        let slot = 0;
        let active_slot_coefficient = 0.05;
        let total_stake = 1000;

        let leaves = note_commitment_leaves(&notes);

        let input = InputWitness {
            note,
            nf_sk,
            cm_path: cl::merkle::path(leaves, 0),
        };

        let mut expected_public_inputs = LeaderPublic::new(
            cl::merkle::root(leaves),
            epoch_nonce,
            slot,
            active_slot_coefficient,
            total_stake,
            input.nullifier(),
            input.evolve_output(b"NOMOS_POL").commit_note(),
        );

        while !expected_public_inputs.check_winning(&input) {
            expected_public_inputs.slot += 1;
        }

        println!("slot={}", expected_public_inputs.slot);

        let private_inputs = LeaderPrivate { input };

        let proof = prove(expected_public_inputs, private_inputs).unwrap();
        assert!(proof
            .verify(nomos_pol_risc0_proofs::PROOF_OF_LEADERSHIP_ID)
            .is_ok());
        assert_eq!(expected_public_inputs, proof.journal.decode().unwrap());
    }
}

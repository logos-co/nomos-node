use cl::{note::NoteWitness, nullifier::NullifierSecret};
use leader_proof_statements::{LeaderPrivate, LeaderPublic};
use nomos_pol_prover::*;
use rand::thread_rng;

const MAX_NOTE_COMMS: usize = 1 << 8;

// derive-unit(NOMOS_NMO)
pub const NMO_UNIT: [u8; 32] = [
    67, 50, 140, 228, 181, 204, 226, 242, 254, 193, 239, 51, 237, 68, 36, 126, 124, 227, 60, 112,
    223, 195, 146, 236, 5, 21, 42, 215, 48, 122, 25, 195,
];

fn note_commitment_leaves(note_commitments: &[cl::NoteCommitment]) -> [[u8; 32]; MAX_NOTE_COMMS] {
    let note_comm_bytes = Vec::from_iter(note_commitments.iter().map(|c| c.as_bytes().to_vec()));
    let cm_leaves = cl::merkle::padded_leaves::<MAX_NOTE_COMMS>(&note_comm_bytes);
    cm_leaves
}

fn main() {
    let mut rng = thread_rng();

    let input = cl::InputWitness {
        note: NoteWitness::basic(32, NMO_UNIT, &mut rng),
        nf_sk: NullifierSecret::random(&mut rng),
    };

    let notes = vec![input.note_commitment()];
    let epoch_nonce = [0u8; 32];
    let slot = 0;
    let active_slot_coefficient = 0.05;
    let total_stake = 1000;

    let leaves = note_commitment_leaves(&notes);
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

    let private_inputs = LeaderPrivate {
        input: input.clone(),
        input_cm_path: cl::merkle::path(leaves, 0),
    };

    let start = std::time::Instant::now();
    let proof = prove(expected_public_inputs, private_inputs).unwrap();
    println!("Prover time: {:?}", start.elapsed());
    let start = std::time::Instant::now();
    assert!(proof
        .verify(nomos_pol_risc0_proofs::PROOF_OF_LEADERSHIP_ID)
        .is_ok());
    println!("Verifier time: {:?}", start.elapsed());
    assert_eq!(expected_public_inputs, proof.journal.decode().unwrap());
}

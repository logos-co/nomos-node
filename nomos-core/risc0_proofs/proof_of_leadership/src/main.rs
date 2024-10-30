use cl::balance::Unit;
/// Proof of Leadership
use cl::merkle;
use nomos_proof_statements::leadership::{LeaderPrivate, LeaderPublic};
use risc0_zkvm::guest::env;

// derive-unit(NOMOS_NMO)
pub const NMO_UNIT: Unit = [
    67, 50, 140, 228, 181, 204, 226, 242, 254, 193, 239, 51, 237, 68, 36, 126, 124, 227, 60, 112,
    223, 195, 146, 236, 5, 21, 42, 215, 48, 122, 25, 195,
];

fn main() {
    let public_inputs: LeaderPublic = env::read();

    let LeaderPrivate { input } = env::read();

    // Lottery checks
    assert!(public_inputs.check_winning(&input));

    // Ensure note is valid
    assert_eq!(input.note.unit, NMO_UNIT);
    let note_cm = input.note_commitment();
    let note_cm_leaf = merkle::leaf(note_cm.as_bytes());
    let note_cm_root = merkle::path_root(note_cm_leaf, &input.cm_path);
    assert_eq!(note_cm_root, public_inputs.cm_root);

    // Public input constraints
    assert_eq!(input.nullifier(), public_inputs.nullifier);

    let evolved_output = input.evolve_output(b"NOMOS_POL");
    assert_eq!(
        evolved_output.commit_note(),
        public_inputs.evolved_commitment
    );

    env::commit(&public_inputs);
}

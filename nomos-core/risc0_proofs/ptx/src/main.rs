/// Input Proof
use cl::merkle;
use nomos_proof_statements::ptx::{PtxPrivate, PtxPublic};
use risc0_zkvm::guest::env;

fn main() {
    let PtxPrivate {
        ptx,
        input_cm_paths,
        cm_root,
    } = env::read();

    assert_eq!(ptx.inputs.len(), input_cm_paths.len());
    for (input, cm_path) in ptx.inputs.iter().zip(input_cm_paths) {
        let note_cm = input.note_commitment();
        let cm_leaf = merkle::leaf(note_cm.as_bytes());
        assert_eq!(cm_root, merkle::path_root(cm_leaf, &cm_path));
    }

    for output in ptx.outputs.iter() {
        assert!(output.note.value > 0);
    }

    env::commit(&PtxPublic {
        ptx: ptx.commit(),
        cm_root,
    });
}

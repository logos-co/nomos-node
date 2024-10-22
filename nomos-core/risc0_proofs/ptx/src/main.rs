use cl::merkle;
use nomos_proof_statements::ptx::{PtxPrivate, PtxPublic};
use risc0_zkvm::guest::env;

fn main() {
    let PtxPrivate { ptx, cm_root } = env::read();

    for input in ptx.inputs.iter() {
        let note_cm = input.note_commitment();
        let cm_leaf = merkle::leaf(note_cm.as_bytes());
        assert_eq!(cm_root, merkle::path_root(cm_leaf, &input.cm_path));
    }

    for output in ptx.outputs.iter() {
        assert!(output.note.value > 0);
    }

    env::commit(&PtxPublic {
        ptx: ptx.commit(),
        cm_root,
    });
}

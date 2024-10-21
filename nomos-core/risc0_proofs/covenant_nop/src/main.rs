/// Constraint No-op Proof
use nomos_proof_statements::covenant::CovenantPublic;
use risc0_zkvm::guest::env;

fn main() {
    let public: CovenantPublic = env::read();
    env::commit(&public);
}

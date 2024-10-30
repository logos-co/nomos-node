/// Bundle Proof
///
/// The bundle proof demonstrates that the set of partial transactions
/// balance to zero. i.e. \sum inputs = \sum outputs.
use risc0_zkvm::guest::env;

fn main() {
    let bundle_private: nomos_proof_statements::bundle::BundlePrivate = env::read();

    let bundle_public = nomos_proof_statements::bundle::BundlePublic {
        balances: Vec::from_iter(bundle_private.balances.iter().map(|b| b.commit())),
    };

    assert!(cl::BalanceWitness::combine(bundle_private.balances, [0u8; 16]).is_zero());

    env::commit(&bundle_public);
}

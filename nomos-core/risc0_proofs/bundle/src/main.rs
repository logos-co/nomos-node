/// Bundle Proof
///
/// The bundle proof demonstrates that the set of partial transactions
/// balance to zero. i.e. \sum inputs = \sum outputs.
///
/// This is done by proving knowledge of some blinding factor `r` s.t.
///     \sum outputs - \sum input = 0*G + r*H
///
/// To avoid doing costly ECC in stark, we compute only the RHS in stark.
/// The sums and equality is checked outside of stark during proof verification.
use risc0_zkvm::guest::env;

fn main() {
    let bundle_private: nomos_proof_statements::bundle::BundlePrivate = env::read();

    let bundle_public = nomos_proof_statements::bundle::BundlePublic {
        balances: Vec::from_iter(bundle_private.balances.iter().map(|b| b.commit())),
    };

    assert!(cl::BalanceWitness::combine(bundle_private.balances, [0u8; 16]).is_zero());

    env::commit(&bundle_public);
}

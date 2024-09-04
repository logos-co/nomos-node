/// Proof of Equivalence
use equivalence_proof_statements::{EquivalencePrivate, EquivalencePublic};
use risc0_zkvm::guest::env;
use ark_bls12_381::Fr;
use ark_ff::{PrimeField};

fn main() {
    let public_inputs: EquivalencePublic = env::read();

    let EquivalencePrivate {
        coefficients,
    } = env::read();
    let private_inputs = EquivalencePrivate { coefficients };

    //compute random point
    let random_point = private_inputs.get_random_point(public_inputs.da_commitment.clone());

    //evaluate the polynomial over BLS
    let evaluation = private_inputs.evaluate(random_point);
    assert_eq!(evaluation, Fr::from_be_bytes_mod_order(&public_inputs.y_0));

    env::commit(&public_inputs);
}

/// Proof of Equivalence
use equivalence_proof_statements::{EquivalencePrivate, EquivalencePublic};
use risc0_zkvm::guest::env;
use sha2::{Digest, Sha256};
use crypto_bigint::{U256, impl_modulus, const_residue, modular::constant_mod::ResidueParams, Encoding};

const BLOB_SIZE: usize = 2048;

impl_modulus!(
    Fr,
    U256,
    "73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001"
);

fn mul_mod(a: U256, b: U256) -> U256 {
    let a = const_residue!(a, Fr);
    let b = const_residue!(b, Fr);
    a.mul(&b).retrieve()
}

fn main() {
    let start_start = env::cycle_count();
    let public_inputs: EquivalencePublic = env::read();

    let EquivalencePrivate {
        coefficients,
    } = env::read();
    let private_inputs = EquivalencePrivate { coefficients };
    let end = env::cycle_count();
    eprintln!("inputs load: {}", end - start_start);

    let start = env::cycle_count();
    // BLS scalar field modulus
    let modulus = U256::from_be_slice(&[115, 237, 167, 83, 41, 157, 125, 72, 51, 57, 216, 8, 9, 161, 216, 5, 83, 189, 164, 2, 255, 254, 91, 254, 255, 255, 255, 255, 0, 0, 0, 1]);
    let end = env::cycle_count();
    eprintln!("modulus conversion from u8: {}", end - start);

    //compute random point
    let start = env::cycle_count();
    let mut hasher = Sha256::new();
    hasher.update(public_inputs.da_commitment.clone());
    for i in 0..BLOB_SIZE {
        hasher.update(private_inputs.coefficients[i].to_be_bytes());
    }
    let x_0 : [u8; 32] = hasher.finalize().into();
    let end = env::cycle_count();
    eprintln!("draw random point: {}", end - start);


    //evaluate the polynomial over BLS
    let start = env::cycle_count();
    let bls_point = U256::from_be_slice(&x_0);
    let end = env::cycle_count();
    eprintln!("evaluation point conversion from u8: {}", end - start);

    let start = env::cycle_count();
    let mut evaluation = private_inputs.coefficients[BLOB_SIZE-1];
    for i in 1..BLOB_SIZE {
        let mul = mul_mod(evaluation, bls_point);
        evaluation = private_inputs.coefficients[BLOB_SIZE-1-i].add_mod(&mul, &modulus);
    }
    let end = env::cycle_count();
    eprintln!("point evaluation: {}", end - start);


    let start = env::cycle_count();
    assert_eq!(evaluation, public_inputs.y_0);
    let end = env::cycle_count();
    eprintln!("last assertion: {}", end - start);

    let start = env::cycle_count();
    env::commit(&public_inputs);
    let end_end = env::cycle_count();
    eprintln!("public input: {}", end_end - start);

    eprintln!("total: {}", end_end - start_start);

}

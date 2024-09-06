/// Proof of Equivalence
use equivalence_proof_statements::{EquivalencePrivate, EquivalencePublic};
use risc0_zkvm::guest::env;
use ark_bls12_381::Fr;
use ark_ff::{PrimeField, BigInteger};
use sha2::{Digest, Sha256};
use ark_poly::univariate::DensePolynomial;
use ark_poly::{DenseUVPolynomial, Polynomial};

fn main() {
    let public_inputs: EquivalencePublic = env::read();

    let EquivalencePrivate {
        coefficients,
    } = env::read();
    let private_inputs = EquivalencePrivate { coefficients };


    //compute random point
    let start = env::cycle_count();
    let mut hasher = Sha256::new();
    hasher.update(public_inputs.da_commitment.clone());
    let x_0 : [u8; 32] = hasher.finalize().into();
    let end = env::cycle_count();
    eprintln!("draw random point: {}", end - start);

    //evaluate the polynomial over BLS
    let start = env::cycle_count();
    let bls_point = Fr::from_be_bytes_mod_order(&x_0);
    let end = env::cycle_count();
    eprintln!("point conversion from u8: {}", end - start);

    let start = env::cycle_count();
    let mut bls_coefficients : Vec<Fr> = vec![];
    for i in 0..private_inputs.coefficients.len() {
        bls_coefficients.push(Fr::from_be_bytes_mod_order(&private_inputs.coefficients[i]));
    }
    let end = env::cycle_count();
    eprintln!("coefficients conversion from u8: {}", end - start);

    let start = env::cycle_count();
    let polynomial = DensePolynomial::from_coefficients_vec(bls_coefficients);
    let end = env::cycle_count();
    eprintln!("polynomial construction: {}", end - start);

    let start = env::cycle_count();
    let evaluation = polynomial.evaluate(&bls_point);
    let end = env::cycle_count();
    eprintln!("point evaluation: {}", end - start);
    let start = env::cycle_count();
    assert_eq!(evaluation, Fr::from_be_bytes_mod_order(&public_inputs.y_0));
    let end = env::cycle_count();
    eprintln!("last assertion: {}", end - start);

    let start = env::cycle_count();
    env::commit(&public_inputs);
    let end = env::cycle_count();
    eprintln!("public input: {}", end - start);

}

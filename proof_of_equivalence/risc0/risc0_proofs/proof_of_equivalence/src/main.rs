/// Proof of Equivalence
use risc0_zkvm::guest::env;
use sha2::{Digest, Sha256};
use crypto_bigint::{U256, impl_modulus, const_residue, modular::constant_mod::ResidueParams};

const BLOB_SIZE: usize = 2048;
const COMMITMENT_SIZE: usize = 48;
const SCALAR_SIZE: usize = 32;

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



fn horner(coefficients: Vec<u8>, point: U256, modulus: U256) -> U256 {
    let mut evaluation = U256::from_be_slice(&coefficients[coefficients.len()-32..coefficients.len()].to_vec());
    for i in 1..BLOB_SIZE {
        let mul = mul_mod(evaluation, point);
        evaluation = U256::from_be_slice(&coefficients[coefficients.len() - 32*(i+1)..coefficients.len() - i*32].to_vec()).add_mod(&mul, &modulus);
    }
    evaluation
}


fn main() {
    let start_start = env::cycle_count();

    let mut inputs = vec![0u8;COMMITMENT_SIZE+SCALAR_SIZE+BLOB_SIZE*32];
    env::read_slice(&mut inputs);
    let da_commitment = inputs[0..COMMITMENT_SIZE].to_vec();
    let y_0 = U256::from_be_slice(& inputs[COMMITMENT_SIZE..COMMITMENT_SIZE+SCALAR_SIZE].to_vec());

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
    hasher.update(da_commitment.clone());
    hasher.update(inputs[COMMITMENT_SIZE+SCALAR_SIZE..COMMITMENT_SIZE+SCALAR_SIZE+BLOB_SIZE*32].to_vec());
    let x_0 : [u8; 32] = hasher.finalize().into();
    let end = env::cycle_count();
    eprintln!("draw random point: {}", end - start);


    //evaluate the polynomial over BLS
    let start = env::cycle_count();
    let bls_point = U256::from_be_slice(&x_0);
    let end = env::cycle_count();
    eprintln!("evaluation point conversion from u8: {}", end - start);

    let start = env::cycle_count();
    //load coefficient and evaluate polynomial with Horner's method
    let evaluation = horner(inputs[COMMITMENT_SIZE+SCALAR_SIZE..COMMITMENT_SIZE+SCALAR_SIZE+BLOB_SIZE*32].to_vec(),bls_point, modulus);


    let end = env::cycle_count();
    eprintln!("point evaluation: {}", end - start);


    let start = env::cycle_count();
    assert_eq!(evaluation, y_0);
    let end = env::cycle_count();
    eprintln!("last assertion: {}", end - start);

    let start = env::cycle_count();
    env::commit(&da_commitment);
    env::commit(&y_0);
    let end_end = env::cycle_count();
    eprintln!("public input: {}", end_end - start);
    eprintln!("total: {}", end_end - start_start);

}

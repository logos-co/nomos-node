use ark_bn254::{Bn254, Fr};
use ark_groth16::Proof;
use risc0_zkvm::guest::env;
use ark_serialize::CanonicalDeserialize;

const N_INPUTS : usize = 1;
const BUFFER_SIZE : usize = 35642 + 128 + 40 * N_INPUTS;
fn main() {
    let start_start = env::cycle_count();
    let mut inputs = vec![0u8; BUFFER_SIZE];
    env::read_slice(&mut inputs);
    let end = env::cycle_count();
    eprintln!("load inputs: {}", end - start_start);

    let start = env::cycle_count();
    let verification_key = ark_groth16::PreparedVerifyingKey::deserialize_compressed_unchecked(&inputs[0..35642]).unwrap();
    let end = env::cycle_count();
    eprintln!("load verification key: {}", end - start);

    let start = env::cycle_count();
    let proof = Proof::deserialize_compressed_unchecked(&inputs[35642..35770]).unwrap();
    let end = env::cycle_count();
    eprintln!("proof load: {}", end - start);

    let start = env::cycle_count();
    let public : Vec<Fr> = Vec::deserialize_compressed_unchecked(&inputs[35770..BUFFER_SIZE]).unwrap();
    let end = env::cycle_count();
    eprintln!("public input load: {}", end - start);

    let start = env::cycle_count();
    // BLS scalar field modulus
    let test = ark_groth16::Groth16::<Bn254>::verify_proof(&verification_key, &proof, &public).unwrap();
    let end = env::cycle_count();
    eprintln!("proof verification: {}", end - start);

    let start = env::cycle_count();
    assert_eq!(test, true);
    let end_end = env::cycle_count();
    eprintln!("test bool: {}", end_end - start);
    eprintln!("total: {}", end_end - start_start);
}

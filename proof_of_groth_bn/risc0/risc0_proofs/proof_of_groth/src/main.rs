use ark_bn254::{Bn254, Fr};
use ark_groth16::Proof;
use risc0_zkvm::guest::env;
use ark_serialize::CanonicalDeserialize;
use risc0_zkvm::sha::Digestible;

const N_INPUTS : usize = 1;
const BUFFER_SIZE : usize = 35930 + 256 + 40 * N_INPUTS;
fn main() {
    let start_start = env::cycle_count();
    let mut inputs = vec![0u8; BUFFER_SIZE];
    env::read_slice(&mut inputs);
    let end = env::cycle_count();
    eprintln!("load inputs: {}", end - start_start);

    let start = env::cycle_count();
    let verification_key = ark_groth16::PreparedVerifyingKey::deserialize_uncompressed_unchecked(&inputs[0..35930]).unwrap();
    let end = env::cycle_count();
    eprintln!("load verification key: {}", end - start);

    let start = env::cycle_count();
    let proof = Proof::deserialize_uncompressed_unchecked(&inputs[35930..36186]).unwrap();
    let end = env::cycle_count();
    eprintln!("proof load: {}", end - start);

    let start = env::cycle_count();
    let public : Vec<Fr> = Vec::deserialize_uncompressed_unchecked(&inputs[36186..BUFFER_SIZE]).unwrap();
    let end = env::cycle_count();
    eprintln!("public input load: {}", end - start);

    let start = env::cycle_count();
    // BLS scalar field modulus
    let test = ark_groth16::Groth16::<Bn254>::verify_proof(&verification_key, &proof, &public).unwrap();
    let end = env::cycle_count();
    eprintln!("proof verification: {}", end - start);

    let start = env::cycle_count();
    assert_eq!(test, true);
    env::commit(&(inputs[0..35930].digest(), inputs[36186..BUFFER_SIZE].digest()));
    let end_end = env::cycle_count();
    eprintln!("test bool: {}", end_end - start);
    eprintln!("total: {}", end_end - start_start);
}

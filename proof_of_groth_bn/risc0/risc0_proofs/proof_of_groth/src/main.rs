use ark_bn254::{Bn254, Fr, G1Affine};
use ark_ff::{BigInteger, Field, PrimeField, Zero};
use ark_groth16::{PreparedVerifyingKey, Proof};
use risc0_zkvm::guest::env;
use ark_serialize::CanonicalDeserialize;
use risc0_zkvm::sha::rust_crypto::{Digest, Sha256};
use core::ops::AddAssign;
use std::ops::{Add, Div, Mul, MulAssign};
use ark_ec::{pairing::Pairing, AffineRepr, CurveGroup, PrimeGroup};
use ark_ec::pairing::PairingOutput;
use ark_groth16::r1cs_to_qap::LibsnarkReduction;

const N_INPUTS : usize = 1;
const N_PROOFS : usize = 3;
const BUFFER_SIZE : usize = 34394 + (256 + 40 * N_INPUTS)*N_PROOFS;
fn main() {
    let start_start = env::cycle_count();
    let mut inputs = vec![0u8; BUFFER_SIZE];
    env::read_slice(&mut inputs);
    let end = env::cycle_count();
    eprintln!("load inputs: {}", end - start_start);

    let start = env::cycle_count();
    let verification_key : PreparedVerifyingKey<Bn254> = ark_groth16::PreparedVerifyingKey::deserialize_uncompressed_unchecked(&inputs[0..34394]).unwrap();
    let end = env::cycle_count();
    eprintln!("load verification key: {}", end - start);

    let start = env::cycle_count();
    let proof_1 : Proof<Bn254> = Proof::deserialize_uncompressed_unchecked(&inputs[34394..34650]).unwrap();
    let proof_2 : Proof<Bn254> = Proof::deserialize_uncompressed_unchecked(&inputs[34690..34946]).unwrap();
    let proof_3 : Proof<Bn254> = Proof::deserialize_uncompressed_unchecked(&inputs[34986..35242]).unwrap();
    let end = env::cycle_count();
    eprintln!("proof load: {}", end - start);

    let start = env::cycle_count();
    let public_1 : Vec<Fr> = Vec::deserialize_uncompressed_unchecked(&inputs[34650..34690]).unwrap();
    let public_2 : Vec<Fr> = Vec::deserialize_uncompressed_unchecked(&inputs[34946..34986]).unwrap();
    let public_3 : Vec<Fr> = Vec::deserialize_uncompressed_unchecked(&inputs[35242..35282]).unwrap();
    let end = env::cycle_count();
    eprintln!("public input load: {}", end - start);

    let start = env::cycle_count();
    let mut hasher = Sha256::new();
    hasher.update(&inputs);
    let r = Fr::from_le_bytes_mod_order(&hasher.finalize());
    let r_2 = r.square().into_bigint();
    let r = r.into_bigint();
    let end = env::cycle_count();
    eprintln!("Fiat Shamir : {}", end - start);

    let start = env::cycle_count();
    // BLS scalar field modulus
    assert_eq!((public_1.len() + 1) ,verification_key.vk.gamma_abc_g1.len());
    assert_eq!((public_2.len() + 1) ,verification_key.vk.gamma_abc_g1.len());
    assert_eq!((public_3.len() + 1) ,verification_key.vk.gamma_abc_g1.len());

    let start = env::cycle_count();
    let mut g_ic_1 : <Bn254 as Pairing>::G1 = (verification_key.vk.gamma_abc_g1[0] as <Bn254 as Pairing>::G1Affine).into_group();
    for (i, b) in public_1.iter().zip(verification_key.vk.gamma_abc_g1.iter().skip(1)) {
        g_ic_1.add_assign(&(b as &<Bn254 as Pairing>::G1Affine).mul_bigint(i.into_bigint()));
    }
    let mut g_ic_2 : <Bn254 as Pairing>::G1 = (verification_key.vk.gamma_abc_g1[0] as <Bn254 as Pairing>::G1Affine).into_group();
    for (i, b) in public_2.iter().zip(verification_key.vk.gamma_abc_g1.iter().skip(1)) {
        g_ic_2.add_assign(&(b as &<Bn254 as Pairing>::G1Affine).mul_bigint(i.into_bigint()));
    }
    let mut g_ic_3 : <Bn254 as Pairing>::G1 = (verification_key.vk.gamma_abc_g1[0] as <Bn254 as Pairing>::G1Affine).into_group();
    for (i, b) in public_3.iter().zip(verification_key.vk.gamma_abc_g1.iter().skip(1)) {
        g_ic_3.add_assign(&(b as &<Bn254 as Pairing>::G1Affine).mul_bigint(i.into_bigint()));
    }
    let g_ic = g_ic_1 + g_ic_2.mul_bigint(r) + g_ic_3.mul_bigint(r_2);
    let end = env::cycle_count();
    eprintln!("public computation : {}", end - start);

    let start = env::cycle_count();
    let qap = Bn254::multi_miller_loop(
        [
            <<Bn254 as Pairing>::G1Affine as Into<<Bn254 as Pairing>::G1Prepared>>::into(proof_1.a),
            g_ic.into_affine().into(),
            (proof_1.c + proof_2.c.mul_bigint(r) + proof_3.c.mul_bigint(r_2)).into(),
        ],
        [
            proof_1.b.into(),
            verification_key.gamma_g2_neg_pc.clone(),
            verification_key.delta_g2_neg_pc.clone(),
        ],
    );
    let end = env::cycle_count();
    eprintln!("Miller loop : {}", end - start);

    let start = env::cycle_count();
    let p = <Bn254 as Pairing>::pairing(proof_2.a.mul_bigint(r),proof_2.b);
    let p_2 = <Bn254 as Pairing>::pairing(proof_3.a.mul_bigint(r_2),proof_3.b);
    let end = env::cycle_count();
    eprintln!("One pairing : {}", end - start);

    let start = env::cycle_count();
    let test = (Bn254::final_exponentiation(qap).unwrap() + p + p_2).mul_bigint((Fr::from_bigint(r_2).unwrap() + Fr::from_bigint(r).unwrap() + Fr::from(1)).inverse().unwrap().into_bigint());
    let end = env::cycle_count();
    eprintln!("final result of pairing sum : {}", end - start);

    let start = env::cycle_count();
    assert_eq!(test.0, verification_key.alpha_g1_beta_g2);

    let end_end = env::cycle_count();
    eprintln!("verification key combination: {}", end_end - start);

    eprintln!("total: {}", end_end - start_start);
}

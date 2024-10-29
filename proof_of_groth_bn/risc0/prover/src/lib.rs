use risc0_zkvm::Receipt;
use std::str::FromStr;
use ark_bn254::{Bn254, Fq, Fq2, Fr};
use thiserror::Error;
use ark_groth16;
use ark_groth16::Proof;
use ark_serialize::CanonicalSerialize;
use serde::{Deserialize, Serialize};
use risc0_zkvm::sha::{Digest, Digestible};

const PROOF: &str = include_str!("../data/proof.json");
const PUBLIC: &str = include_str!("../data/public.json");
const VERIFICATION_KEY: &str = include_str!("../data/verification_key.json");

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to produce proof")]
    ProofError(#[from] anyhow::Error),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VerifyingKeyJson {
    protocol: String,
    curve: String,
    #[serde(rename = "nPublic")]
    n_public: u32,
    vk_alpha_1: Vec<String>,
    vk_beta_2: Vec<Vec<String>>,
    vk_gamma_2: Vec<Vec<String>>,
    vk_delta_2: Vec<Vec<String>>,
    vk_alphabeta_12: Vec<Vec<Vec<String>>>,
    #[serde(rename = "IC")]
    ic: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProofJson {
    pi_a: Vec<String>,
    pi_b: Vec<Vec<String>>,
    pi_c: Vec<String>,
    protocol: Option<String>,
    curve: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PublicInputsJson {
    /// values of the public witness
    pub values: Vec<String>,
}

pub fn prove() -> Result<Receipt, Error> {

    let proof_json: ProofJson = serde_json::from_str(PROOF).unwrap();
    let public_inputs_json = PublicInputsJson {
        values: serde_json::from_str(PUBLIC).unwrap(),
    };
    let verifying_key_json: VerifyingKeyJson = serde_json::from_str(VERIFICATION_KEY).unwrap();

    let verification_key: ark_groth16::PreparedVerifyingKey<Bn254> = ark_groth16::verifier::prepare_verifying_key(&ark_groth16::VerifyingKey {
        alpha_g1: ark_bn254::G1Affine::new(
            Fq::from_str(
                &verifying_key_json.vk_alpha_1[0]
            )
                .unwrap(),
            Fq::from_str(
                &verifying_key_json.vk_alpha_1[1]
            )
                .unwrap(),
        ),
        beta_g2: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    &verifying_key_json.vk_beta_2[0][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &verifying_key_json.vk_beta_2[0][1]
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    &verifying_key_json.vk_beta_2[1][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &verifying_key_json.vk_beta_2[1][1]
                )
                    .unwrap(),
            ),
        ),
        gamma_g2: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    &verifying_key_json.vk_gamma_2[0][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &verifying_key_json.vk_gamma_2[0][1]
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    &verifying_key_json.vk_gamma_2[1][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &verifying_key_json.vk_gamma_2[1][1]
                )
                    .unwrap(),
            ),
        ),
        delta_g2: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    &verifying_key_json.vk_delta_2[0][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &verifying_key_json.vk_delta_2[0][1]
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    &verifying_key_json.vk_delta_2[1][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &verifying_key_json.vk_delta_2[1][1]
                )
                    .unwrap(),
            ),
        ),
        gamma_abc_g1: verifying_key_json.ic.iter().map(|x| ark_bn254::G1Affine::new(
            Fq::from_str(
                &x[0],
            )
                .unwrap(),
            Fq::from_str(
                &x[1]
            )
                .unwrap(),
        )).collect()});


    let mut vec_verif_key: Vec<u8> = vec![];
    verification_key.serialize_uncompressed(&mut vec_verif_key).unwrap();

    let proof: Proof<Bn254> = Proof {
        a: ark_bn254::G1Affine::new(
            Fq::from_str(
                &proof_json.pi_a[0]
            )
                .unwrap(),
            Fq::from_str(
                &proof_json.pi_a[1]
            )
                .unwrap(),
        ),
        b: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    &proof_json.pi_b[0][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &proof_json.pi_b[0][1]
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    &proof_json.pi_b[1][0]
                )
                    .unwrap(),
                Fq::from_str(
                    &proof_json.pi_b[1][1]
                )
                    .unwrap(),
            ),
        ),
        c: ark_bn254::G1Affine::new(
            Fq::from_str(
                &proof_json.pi_c[0]
            )
                .unwrap(),
            Fq::from_str(
                &proof_json.pi_c[1]
            )
                .unwrap(),
        ),
    };
    let mut vec_proof: Vec<u8> = vec![];
    proof.serialize_uncompressed(&mut vec_proof).unwrap();


    let public: Vec<Fr> = public_inputs_json.values.iter().map(|x| Fr::from_str(
            &x,
        )
            .unwrap()    ).collect();
    let mut vec_public: Vec<u8> = vec![];
    public.serialize_uncompressed(&mut vec_public).unwrap();

    let mut buffer: Vec<u8> = vec![];
    buffer.append(&mut vec_verif_key);
    eprintln!("buffer size {}",buffer.len());
    buffer.append(&mut vec_proof);
    eprintln!("buffer size {}",buffer.len());
    buffer.append(&mut vec_public);
    eprintln!("buffer size {}",buffer.len());

    let env = risc0_zkvm::ExecutorEnv::builder()
        .write_slice(&buffer)
        .build()
        .unwrap();

    // Obtain the default prover.
    let prover = risc0_zkvm::default_prover();

    let start_t = std::time::Instant::now();

    // Proof information by proving the specified ELF binary.
    // This struct contains the receipt along with statistics about execution of the guest
    let opts = risc0_zkvm::ProverOpts::succinct();
    let prove_info =
        prover.prove_with_opts(env, nomos_groth_risc0_proofs::PROOF_OF_GROTH_ELF, &opts)?;

    println!(
        "STARK prover time: {:.2?}, total_cycles: {}",
        start_t.elapsed(),
        prove_info.stats.total_cycles
    );

    // Decode the verifier key digest and public input digest from the guest
    let (committed_vk_digest, committed_input_digest): (Digest, Digest) =
        prove_info.receipt.journal.decode().unwrap();

    assert_eq!(committed_vk_digest, vec_verif_key.digest());
    assert_eq!(committed_input_digest, vec_public.digest());

    // extract the receipt.
    Ok(prove_info.receipt)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_groth_prover() {
        let proof = prove().unwrap();
        assert!(proof
            .verify(nomos_groth_risc0_proofs::PROOF_OF_GROTH_ID)
            .is_ok());
    }
}

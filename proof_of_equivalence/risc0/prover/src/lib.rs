use equivalence_proof_statements::{EquivalencePrivate, EquivalencePublic};
use risc0_zkvm::Receipt;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to produce proof")]
    ProofError(#[from] anyhow::Error),
}

pub fn prove(equivalence_public: EquivalencePublic, equivalence_private: EquivalencePrivate) -> Result<Receipt, Error> {
    let env = risc0_zkvm::ExecutorEnv::builder()
        .write(&equivalence_public)
        .unwrap()
        .write(&equivalence_private)
        .unwrap()
        .build()
        .unwrap();

    // Obtain the default prover.
    let prover = risc0_zkvm::default_prover();

    let start_t = std::time::Instant::now();

    // Proof information by proving the specified ELF binary.
    // This struct contains the receipt along with statistics about execution of the guest
    let opts = risc0_zkvm::ProverOpts::succinct();
    let prove_info =
        prover.prove_with_opts(env, nomos_poe_risc0_proofs::PROOF_OF_EQUIVALENCE_ELF, &opts)?;

    println!(
        "STARK prover time: {:.2?}, total_cycles: {}",
        start_t.elapsed(),
        prove_info.stats.total_cycles
    );
    // extract the receipt.
    Ok(prove_info.receipt)
}

#[cfg(test)]
mod test {
    use super::*;
    use ark_bls12_381::{Bls12_381, Fr};
    use ark_ff::{BigInteger, PrimeField};
    use kzgrs::kzg;
    use once_cell::sync::Lazy;
    use ark_poly_commit::kzg10::{KZG10, Proof, UniversalParams};
    use ark_poly::univariate::DensePolynomial;
    use ark_poly::{DenseUVPolynomial, Polynomial};
    use ark_serialize::CanonicalSerialize;
    use sha2::{Digest, Sha256};
    use std::ops::{Mul, Neg};
    use ark_ec::pairing::Pairing;

    const BLOB_SIZE: usize = 32;

    static GLOBAL_PARAMETERS: Lazy<UniversalParams<Bls12_381>> = Lazy::new(|| {
        let mut rng = rand::thread_rng();
        KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(64, true, &mut rng).unwrap()
    });

    #[test]
    fn test_equivalence_prover() {

        let mut coefficients = vec![[0u8; 32];BLOB_SIZE];
        coefficients[0] = [3, 197, 166, 75, 24, 139, 38, 74, 51, 123, 5, 246, 166, 244, 77, 103, 24, 186, 133, 89, 8, 192, 11, 91, 43, 179, 28, 107, 27, 204, 250, 101];
        coefficients[1] = [5, 249, 189, 55, 246, 170, 218, 206, 55, 177, 124, 243, 213, 172, 141, 12, 125, 168, 119, 125, 140, 217, 13, 232, 83, 90, 156, 52, 197, 237, 34, 242];
        coefficients[2] = [1, 27, 246, 13, 198, 26, 152, 229, 122, 151, 244, 159, 178, 241, 189, 146, 66, 150, 64, 228, 86, 247, 65, 41, 200, 118, 27, 233, 3, 173, 182, 59];

        let mut bls_coefficients = vec![];
        for i in 0..BLOB_SIZE {
            bls_coefficients.push(Fr::from_be_bytes_mod_order(&coefficients[i]));
        }

        let mut da_commitment = Vec::new();
        let bls_polynomial = DensePolynomial::from_coefficients_vec(bls_coefficients);
        let bls_polynomial_commitment = kzg::commit_polynomial(&bls_polynomial,&GLOBAL_PARAMETERS).unwrap();
        bls_polynomial_commitment.0.serialize_compressed(&mut da_commitment).unwrap();

        //recover x_0
        let mut hasher = Sha256::new();
        hasher.update(da_commitment.clone());
        for i in 0..coefficients.clone().len() {
            hasher.update(coefficients[i].clone());
        }
        let x_0 = Fr::from_be_bytes_mod_order(&hasher.finalize());

        let y_0 = bls_polynomial.evaluate(&x_0); // EVAL OF x0
        let f_x_y_0 = bls_polynomial + DensePolynomial::<Fr>::from_coefficients_vec(vec![-y_0]); // f(x) - y_0
        let x_x_0 = DensePolynomial::<Fr>::from_coefficients_vec(vec![-x_0, Fr::from(1)]); // X - x_0
        let witness_polynomial: DensePolynomial<_> = &f_x_y_0 / &x_x_0;
        let kzg_proof = kzg::commit_polynomial(&witness_polynomial, &GLOBAL_PARAMETERS).unwrap();
        let kzg_proof : ark_poly_commit::kzg10::Proof<Bls12_381> = Proof {
            w: kzg_proof.0,
            random_v: None,
        };

        let commitment_check_g1 = bls_polynomial_commitment.0 + GLOBAL_PARAMETERS.powers_of_g[0].mul(y_0).neg(); // replace y_0 with risc0 result
        let proof_check_g2 = GLOBAL_PARAMETERS.beta_h + GLOBAL_PARAMETERS.h.mul(x_0).neg();
        let lhs = Bls12_381::pairing(commitment_check_g1, GLOBAL_PARAMETERS.h);
        let rhs = Bls12_381::pairing(kzg_proof.w, proof_check_g2);
        assert_eq!(lhs, rhs);



        let expected_public_inputs = EquivalencePublic::new(
            da_commitment.clone(),
            y_0.into_bigint().to_bytes_be().try_into().unwrap()
        );

        let private_inputs = EquivalencePrivate::new(
            coefficients.clone()
        );

        // Zone STF
        let proof = prove(expected_public_inputs.clone(), private_inputs).unwrap();
        assert!(proof
            .verify(nomos_poe_risc0_proofs::PROOF_OF_EQUIVALENCE_ID)
            .is_ok());
        let journal = proof.journal.decode().unwrap();
        assert_eq!(expected_public_inputs.clone(), journal);
    }
}

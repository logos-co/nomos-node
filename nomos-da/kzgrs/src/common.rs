// std
use std::io::BufReader;
// crates
use ark_bls12_381::{fr::Fr, Bls12_381};
use ark_ec::pairing::Pairing;
use ark_ff::{BigInteger256, FftField, Zero};
use ark_poly::domain::general::GeneralEvaluationDomain;
use ark_poly::evaluations::univariate::Evaluations;
use ark_poly::univariate::DensePolynomial;
use ark_poly::{DenseUVPolynomial, EvaluationDomain, Polynomial};
use ark_serialize::CanonicalDeserialize;
use thiserror::Error;
// internal

#[derive(Error, Debug)]
pub enum KzgRsError {
    #[error("Data isn't properly padded, data len must match modulus {expected_modulus} but it is {current_size}")]
    UnpaddedDataError {
        expected_modulus: usize,
        current_size: usize,
    },
    #[error("ChunkSize should be < 32 (bytes), got {0}")]
    ChunkSizeTooBig(usize),
}

fn bytes_to_polynomial<const CHUNK_SIZE: usize>(
    data: &[u8],
    domain: GeneralEvaluationDomain<Fr>,
) -> Result<DensePolynomial<Fr>, KzgRsError> {
    if CHUNK_SIZE >= 32 {
        return Err(KzgRsError::ChunkSizeTooBig(CHUNK_SIZE));
    }
    if data.len() % CHUNK_SIZE != 0 {
        return Err(KzgRsError::UnpaddedDataError {
            expected_modulus: CHUNK_SIZE,
            current_size: data.len(),
        });
    }
    let coefficients = Evaluations::from_vec_and_domain(
        data.chunks(CHUNK_SIZE)
            .map(|e| {
                let mut buff = [0u8; 32];
                buff[0..31].copy_from_slice(e);
                Fr::new_unchecked(
                    BigInteger256::deserialize_uncompressed(BufReader::new(buff.as_slice()))
                        .unwrap(),
                )
            })
            .collect(),
        domain,
    )
    .interpolate()
    .coeffs
    .into_iter()
    .take(data.len() / CHUNK_SIZE);
    Ok(DensePolynomial::from_coefficients_vec(
        coefficients.collect(),
    ))
}

#[cfg(test)]
mod test {
    use super::{bytes_to_polynomial, KzgRsError};
    use ark_bls12_381::fr::Fr;
    use ark_ec::pairing::Pairing;
    use ark_ff::{BigInteger, PrimeField};
    use ark_poly::{EvaluationDomain, GeneralEvaluationDomain, Polynomial};
    use once_cell::sync::Lazy;
    use rand::{thread_rng, Fill};

    const CHUNK_SIZE: usize = 31;
    static DOMAIN: Lazy<GeneralEvaluationDomain<Fr>> =
        Lazy::new(|| GeneralEvaluationDomain::new(128).unwrap());
    #[test]
    fn encode_random_polynomial() {
        const N: usize = 100;
        let mut bytes: [u8; CHUNK_SIZE * N] = [0; CHUNK_SIZE * N];
        let mut rng = thread_rng();
        bytes.try_fill(&mut rng).unwrap();
        let poly = bytes_to_polynomial::<31>(&bytes, *DOMAIN).unwrap();
        assert_eq!(poly.degree(), N - 1);
        for (i, e) in (0..100).map(|i| 2u32.pow(i)).enumerate() {
            let eval_point = Fr::from(e);
            let point = poly.evaluate(&eval_point);
            let bint = point.into_bigint();
            assert_eq!(
                &bytes[CHUNK_SIZE * i..CHUNK_SIZE * i + CHUNK_SIZE],
                &bint.to_bytes_be()
            )
        }
    }

    #[test]
    fn encode_chunk_size_too_big() {
        assert!(matches!(
            bytes_to_polynomial::<32>(&[], *DOMAIN),
            Err(KzgRsError::ChunkSizeTooBig(32))
        ));
    }

    #[test]
    fn encode_not_padded_data() {
        assert!(matches!(
            bytes_to_polynomial::<31>(&[0; 12], *DOMAIN),
            Err(KzgRsError::UnpaddedDataError {
                expected_modulus: 31,
                current_size: 12
            })
        ));
    }
}

// std
// crates
use ark_bls12_381::fr::Fr;
use ark_ff::{BigInteger256, Zero};
use ark_poly::domain::general::GeneralEvaluationDomain;
use ark_poly::evaluations::univariate::Evaluations;
use ark_poly::univariate::DensePolynomial;
use num_bigint;
use num_traits::ops::bytes::FromBytes;
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

fn bytes_to_evaluations<const CHUNK_SIZE: usize>(
    data: &[u8],
    domain: GeneralEvaluationDomain<Fr>,
) -> Evaluations<Fr> {
    assert!((data.len() % CHUNK_SIZE).is_zero());
    Evaluations::from_vec_and_domain(
        data.chunks(CHUNK_SIZE)
            .map(|e| {
                // use little endian for convenience as shortening 1 byte (<32 supported)
                // do not matter in this endianness
                let bint: BigInteger256 = num_bigint::BigUint::from_le_bytes(e)
                    .try_into()
                    .expect("Bytes size should fit for an 256 bits integer");
                Fr::new(bint)
            })
            .collect(),
        domain,
    )
}

pub fn bytes_to_polynomial<const CHUNK_SIZE: usize>(
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
    let evals = bytes_to_evaluations::<CHUNK_SIZE>(data, domain);
    let coefficients = evals.interpolate();
    Ok(coefficients)
}

#[cfg(test)]
mod test {
    use super::{bytes_to_evaluations, bytes_to_polynomial, KzgRsError};
    use ark_bls12_381::fr::Fr;
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
        let evals = bytes_to_evaluations::<31>(&bytes, *DOMAIN);
        let poly = bytes_to_polynomial::<31>(&bytes, *DOMAIN).unwrap();
        for i in 0..100 {
            let eval_point = DOMAIN.element(i);
            let point = poly.evaluate(&eval_point);
            // check point is the same
            assert_eq!(evals[i as usize], point);
            // check point bytes are the same
            assert_eq!(
                &bytes[CHUNK_SIZE * i..CHUNK_SIZE * i + CHUNK_SIZE],
                &point.into_bigint().to_bytes_le()[..CHUNK_SIZE]
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

// std
use std::io::BufReader;
// crates
use ark_bls12_381::fr::Fr;
use ark_ff::BigInteger256;
use ark_poly::univariate::DensePolynomial;
use ark_poly::{DenseUVPolynomial, Polynomial};
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
) -> Result<impl Polynomial<Fr>, KzgRsError> where {
    if CHUNK_SIZE >= 32 {
        return Err(KzgRsError::ChunkSizeTooBig(CHUNK_SIZE));
    }
    if data.len() % CHUNK_SIZE != 0 {
        return Err(KzgRsError::UnpaddedDataError {
            expected_modulus: CHUNK_SIZE,
            current_size: data.len(),
        });
    }
    Ok(DensePolynomial::from_coefficients_vec(
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
    ))
}

#[cfg(test)]
mod test {
    use super::{bytes_to_polynomial, KzgRsError};
    use ark_poly::Polynomial;
    use rand::{thread_rng, Fill};

    const CHUNK_SIZE: usize = 31;

    #[test]
    fn encode_random_polynomial() {
        const N: usize = 100;
        let mut bytes: [u8; CHUNK_SIZE * N] = [0; CHUNK_SIZE * N];
        let mut rng = thread_rng();
        bytes.try_fill(&mut rng).unwrap();
        let poly = bytes_to_polynomial::<31>(&bytes).unwrap();
        assert_eq!(poly.degree(), N - 1);
    }

    #[test]
    fn encode_chunk_size_too_big() {
        assert!(matches!(
            bytes_to_polynomial::<32>(&[]),
            Err(KzgRsError::ChunkSizeTooBig(32))
        ));
    }

    #[test]
    fn encode_not_padded_data() {
        assert!(matches!(
            bytes_to_polynomial::<31>(&[0; 12]),
            Err(KzgRsError::UnpaddedDataError {
                expected_modulus: 31,
                current_size: 12
            })
        ));
    }
}

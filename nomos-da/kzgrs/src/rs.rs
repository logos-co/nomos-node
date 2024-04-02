use ark_bls12_381::Fr;
use ark_ff::{BigInteger, PrimeField};
use ark_poly::univariate::DensePolynomial;
use ark_poly::{EvaluationDomain, Evaluations, GeneralEvaluationDomain, Polynomial};

pub fn encode(
    polynomial: &DensePolynomial<Fr>,
    evaluations: &Evaluations<Fr>,
    factor: usize,
    domain: &GeneralEvaluationDomain<Fr>,
) -> Evaluations<Fr> {
    assert!(factor > 1);
    Evaluations::from_vec_and_domain(
        (0..evaluations.evals.len() * factor)
            .map(|i| polynomial.evaluate(&domain.element(i)))
            .collect(),
        *domain,
    )
}

pub fn decode(
    original_chunks_len: usize,
    points: &[Fr],
    domain: &GeneralEvaluationDomain<Fr>,
) -> Evaluations<Fr> {
    let evals = Evaluations::<Fr>::from_vec_and_domain(points.to_vec(), *domain);
    let coeffs = evals.interpolate();

    Evaluations::from_vec_and_domain(
        (0..original_chunks_len)
            .map(|i| coeffs.evaluate(&domain.element(i)))
            .collect(),
        *domain,
    )
}

pub fn points_to_bytes<const CHUNK_SIZE: usize>(points: &[Fr]) -> Vec<u8> {
    fn point_to_buff<const CHUNK_SIZE: usize>(p: &Fr) -> impl Iterator<Item = u8> {
        p.into_bigint().to_bytes_le().into_iter().take(CHUNK_SIZE)
    }
    points
        .iter()
        .map(point_to_buff::<CHUNK_SIZE>)
        .flatten()
        .collect()
}

#[cfg(test)]
mod test {
    use crate::common::bytes_to_polynomial;
    use crate::rs::{decode, encode, points_to_bytes};
    use ark_bls12_381::{Bls12_381, Fr};
    use ark_poly::univariate::DensePolynomial;
    use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
    use ark_poly_commit::kzg10::{UniversalParams, KZG10};
    use once_cell::sync::Lazy;
    use rand::{thread_rng, Fill};

    const COEFFICIENTS_SIZE: usize = 16;
    static DOMAIN: Lazy<GeneralEvaluationDomain<Fr>> =
        Lazy::new(|| GeneralEvaluationDomain::new(COEFFICIENTS_SIZE).unwrap());

    #[test]
    fn test_encode_decode() {
        let mut bytes: [u8; 310] = [0; 310];
        let mut rng = thread_rng();
        bytes.try_fill(&mut rng).unwrap();

        let (evals, poly) = bytes_to_polynomial::<31>(&bytes, *DOMAIN).unwrap();

        let mut encoded = encode(&poly, &evals, 2, &DOMAIN);

        let decoded = decode(10, &encoded.evals, &DOMAIN);
        let decoded_bytes = points_to_bytes::<31>(&decoded.evals);
        assert_eq!(decoded_bytes, bytes);
    }
}

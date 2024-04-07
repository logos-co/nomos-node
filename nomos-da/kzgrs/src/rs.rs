use ark_bls12_381::Fr;
use ark_ff::{BigInteger, Field, PrimeField};
use ark_poly::univariate::DensePolynomial;
use ark_poly::{
    DenseUVPolynomial, EvaluationDomain, Evaluations, GeneralEvaluationDomain, Polynomial,
};
use num_traits::Zero;
use std::ops::{Mul, Neg};

/// Extend a polynomial over some factor `polynomial.len()*factor and return the original points
/// plus the extra ones.
/// `factor` need to be `>1`
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

/// Interpolate points into a polynomial, then evaluate the polynomial in the original evaluations
/// to recover the original data.
/// `domain` need to be the same domain of the original `evaluations` and `polynomial` used for encoding.
pub fn decode(
    original_chunks_len: usize,
    points: &[Option<Fr>],
    domain: &GeneralEvaluationDomain<Fr>,
) -> Evaluations<Fr> {
    let (points, roots_of_unity): (Vec<Fr>, Vec<Fr>) = points
        .iter()
        .enumerate()
        .flat_map(|(i, e)| {
            if let Some(e) = e {
                Some((*e, domain.element(i)))
            } else {
                None
            }
        })
        .unzip();
    let coeffs = lagrange_interpolate(&points, &roots_of_unity);
    Evaluations::from_vec_and_domain(
        (0..original_chunks_len)
            .map(|i| coeffs.evaluate(&domain.element(i)))
            .collect(),
        *domain,
    )
}

/// Interpolate a set of points using lagrange interpolation and roots of unity
/// Warning!! Be aware that the mapping between points and roots of unity is the intended:
/// A polynomial `f(x)` is derived for `w_x` (root) mapping to p_x. `[(w_1, p_1)..(w_n, p_n)]` even
/// if points are missing it is important to keep the mapping integrity.
pub fn lagrange_interpolate(points: &[Fr], roots_of_unity: &[Fr]) -> DensePolynomial<Fr> {
    assert_eq!(points.len(), roots_of_unity.len());
    let mut result = DensePolynomial::from_coefficients_vec(vec![Fr::zero()]);
    for i in 0..roots_of_unity.len() {
        let mut summand = DensePolynomial::from_coefficients_vec(vec![points[i]]);
        for j in 0..points.len() {
            if i != j {
                let weight_adjustment =
                    (roots_of_unity[i] - roots_of_unity[j])
                        .inverse()
                        .expect(
                            "Roots of unity are/should not repeated. If this panics it means we have no coefficients enough in the evaluation domain"
                        );
                summand = summand.naive_mul(&DensePolynomial::from_coefficients_vec(vec![
                    weight_adjustment.mul(roots_of_unity[j]).neg(),
                    weight_adjustment,
                ]))
            }
        }
        result = result + summand;
    }
    result
}

/// Reconstruct bytes from the polynomial evaluation points using original chunk size and a set of points
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
    use ark_bls12_381::Fr;
    use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
    use once_cell::sync::Lazy;
    use rand::{thread_rng, Fill};

    const COEFFICIENTS_SIZE: usize = 32;
    static DOMAIN: Lazy<GeneralEvaluationDomain<Fr>> =
        Lazy::new(|| GeneralEvaluationDomain::new(COEFFICIENTS_SIZE).unwrap());

    #[test]
    fn test_encode_decode() {
        let mut bytes: [u8; 310] = [0; 310];
        let mut rng = thread_rng();
        bytes.try_fill(&mut rng).unwrap();

        let (evals, poly) = bytes_to_polynomial::<31>(&bytes, *DOMAIN).unwrap();

        let encoded = encode(&poly, &evals, 2, &DOMAIN);
        let mut encoded: Vec<Option<Fr>> = encoded.evals.into_iter().map(Some).collect();

        let decoded = decode(10, &encoded, &DOMAIN);
        let decoded_bytes = points_to_bytes::<31>(&decoded.evals);
        assert_eq!(decoded_bytes, bytes);

        // check with missing pieces

        for i in (1..encoded.len()).step_by(2) {
            encoded[i] = None;
        }

        let decoded_bytes = points_to_bytes::<31>(&decoded.evals);
        assert_eq!(decoded_bytes, bytes);
    }
}

use ark_bls12_381::{Fr, G1Affine, G1Projective};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{FftField, Field};
use ark_poly::{EvaluationDomain, GeneralEvaluationDomain, Polynomial as _};
use num_traits::Zero;

use crate::common::compute_roots_of_unity;
use crate::fft::{fft_fr, fft_g1, ifft_g1};
use crate::{GlobalParameters, Polynomial, Proof};

fn toeplitz1(global_parameters: &[G1Affine], polynomial_degree: usize) -> Vec<G1Affine> {
    debug_assert_eq!(global_parameters.len(), polynomial_degree);
    debug_assert!(polynomial_degree.is_power_of_two());
    let roots_of_unity = compute_roots_of_unity(polynomial_degree * 2);
    let vector_extended: Vec<G1Affine> = global_parameters
        .iter()
        .copied()
        .chain(std::iter::repeat_with(G1Affine::zero).take(polynomial_degree))
        .collect();
    fft_g1(&vector_extended, &roots_of_unity)
}

fn toeplitz2(coefficients: &[Fr], extended_vector: &[G1Affine]) -> Vec<G1Affine> {
    debug_assert!(coefficients.len().is_power_of_two());
    // let domain: GeneralEvaluationDomain<Fr> =
    //     GeneralEvaluationDomain::new(coefficients.len()).expect("Domain should be able to build");
    // let toeplitz_coefficients_fft = domain.fft(coefficients);
    let roots_of_unity = compute_roots_of_unity(coefficients.len());
    let toeplitz_coefficients_fft = fft_fr(coefficients, &roots_of_unity);
    extended_vector
        .iter()
        .copied()
        .zip(toeplitz_coefficients_fft)
        .map(|(v, c)| (v * c).into_affine())
        .collect()
}

fn toeplitz3(h_extended_fft: &[G1Affine], polynomial_degree: usize) -> Vec<G1Affine> {
    let roots_of_unity: Vec<Fr> = compute_roots_of_unity(h_extended_fft.len());
    ifft_g1(h_extended_fft, &roots_of_unity)
        .into_iter()
        .take(polynomial_degree)
        .collect()
}

pub fn fk20_batch_generate_elements_proofs(
    polynomial: &Polynomial,
    global_parameters: &GlobalParameters,
) -> Vec<Proof> {
    let polynomial_degree = polynomial.len();
    debug_assert!(polynomial_degree <= global_parameters.powers_of_g.len());
    debug_assert!(polynomial_degree.is_power_of_two());
    let roots_of_unity: Vec<Fr> = compute_roots_of_unity(polynomial_degree);
    let global_parameters: Vec<G1Affine> = global_parameters
        .powers_of_g
        .iter()
        .copied()
        .take(polynomial_degree)
        .rev()
        .collect();

    let extended_vector = toeplitz1(&global_parameters, polynomial_degree);
    let toeplitz_coefficients: Vec<Fr> = std::iter::repeat(Fr::ZERO)
        .take(polynomial_degree)
        .chain(polynomial.coeffs.iter().copied())
        .collect();
    let h_extended_vector = toeplitz2(&toeplitz_coefficients, &extended_vector);
    let h_vector = toeplitz3(&h_extended_vector, polynomial_degree);
    fft_g1(&h_vector, &roots_of_unity)
        .into_iter()
        .map(|g1| Proof {
            w: g1,
            random_v: None,
        })
        .collect()
}

#[cfg(test)]
mod test {
    use crate::common::compute_roots_of_unity;
    use crate::fk20::fk20_batch_generate_elements_proofs;
    use crate::{
        common::bytes_to_polynomial, kzg::generate_element_proof, GlobalParameters, Proof,
        BYTES_PER_FIELD_ELEMENT,
    };
    use ark_bls12_381::{Bls12_381, Fr};
    use ark_ff::FftField;
    use ark_poly::univariate::DensePolynomial;
    use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
    use ark_poly_commit::kzg10::KZG10;
    use once_cell::sync::Lazy;
    use rand::SeedableRng;

    static GLOBAL_PARAMETERS: Lazy<GlobalParameters> = Lazy::new(|| {
        let mut rng = rand::rngs::StdRng::seed_from_u64(1987);
        KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(4096, true, &mut rng).unwrap()
    });

    #[test]
    fn test_generate_proofs() {
        for size in [16, 32, 64, 128, 256] {
            let mut buff: Vec<_> = (0..BYTES_PER_FIELD_ELEMENT * size)
                .map(|i| (i % 255) as u8)
                .rev()
                .collect();
            let domain = GeneralEvaluationDomain::new(size).unwrap();
            let (evals, poly) =
                bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(&buff, domain).unwrap();
            let polynomial_degree = poly.len();
            let roots_of_unity: Vec<Fr> = compute_roots_of_unity(size);
            let slow_proofs: Vec<Proof> = (0..polynomial_degree)
                .map(|i| {
                    generate_element_proof(i, &poly, &evals, &GLOBAL_PARAMETERS, domain).unwrap()
                })
                .collect();
            let fk20_proofs = fk20_batch_generate_elements_proofs(&poly, &GLOBAL_PARAMETERS);
            assert_eq!(slow_proofs, fk20_proofs);
        }
    }
}

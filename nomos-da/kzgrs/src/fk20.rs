use ark_bls12_381::{Fr, G1Affine, G1Projective};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::FftField;
use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
use num_traits::Zero;

use crate::fft::{fft_g1, ifft_g1};
use crate::{GlobalParameters, Polynomial, Proof};

fn toeplitz1(global_parameters: &[G1Affine], roots_of_unity: &[Fr]) -> Vec<G1Affine> {
    debug_assert_eq!(global_parameters.len(), roots_of_unity.len());
    debug_assert!(roots_of_unity.len().is_power_of_two());
    let vector_extended: Vec<G1Affine> = global_parameters
        .iter()
        .copied()
        .chain(std::iter::repeat_with(G1Affine::zero).take(global_parameters.len()))
        .collect();
    fft_g1(&vector_extended, &roots_of_unity)
}

fn toeplitz2(coefficients: &[Fr], extended_vector: &[G1Affine]) -> Vec<G1Affine> {
    debug_assert!(coefficients.len().is_power_of_two());
    let domain: GeneralEvaluationDomain<Fr> =
        GeneralEvaluationDomain::new(coefficients.len()).expect("Domain should be able to build");
    let toeplitz_coefficients_fft = domain.fft(coefficients);
    extended_vector
        .iter()
        .copied()
        .zip(toeplitz_coefficients_fft.iter())
        .map(|(v, c)| (v * c).into_affine())
        .collect()
}

fn toeplitz3(h_extended_fft: &[G1Affine], polynomial_degree: usize) -> Vec<G1Affine> {
    let roots_of_unity: Vec<Fr> = (0..h_extended_fft.len())
        .map(|i| Fr::get_root_of_unity(i as u64).expect("Root should be present"))
        .collect();
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
    let roots_of_unity: Vec<Fr> = (0..polynomial_degree)
        .map(|i| Fr::get_root_of_unity(i as u64).expect("Root should be present"))
        .collect();
    let global_parameters: Vec<G1Affine> = global_parameters
        .powers_of_g
        .iter()
        .copied()
        .take(polynomial_degree)
        .rev()
        .chain(std::iter::once(G1Affine::identity() * Fr::from(0)).map(G1Projective::into_affine))
        .collect();

    let extended_vector = toeplitz1(&global_parameters, &roots_of_unity);
    let toeplitz_coefficients: Vec<Fr> = std::iter::once(
        polynomial
            .coeffs
            .last()
            .copied()
            .expect("Polynomial should not be empty"),
    )
    .chain(std::iter::repeat_with(Fr::zero).take(polynomial_degree + 1))
    .chain(polynomial.coeffs.iter().skip(1).rev().copied())
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

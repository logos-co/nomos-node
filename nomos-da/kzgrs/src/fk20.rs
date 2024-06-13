use crate::{GlobalParameters, Polynomial, Proof};
use ark_bls12_381::{Bls12_381, Fq, Fr, G1Affine, G1Projective};
use ark_ec::mnt4::G1Prepared;
use ark_ec::{AffineRepr, CurveGroup};

use ark_poly::univariate::DensePolynomial;
use ark_poly::{EvaluationDomain, Radix2EvaluationDomain};
use num_traits::Zero;
fn toeplitz1(global_parameters: &GlobalParameters, polynomial_degree: usize) -> Vec<G1Projective> {
    debug_assert_eq!(global_parameters.powers_of_g.len(), polynomial_degree);
    debug_assert!(polynomial_degree.is_power_of_two());
    unimplemented!()
}

pub fn fk20_batch_generate_elements_proofs(
    polynomial: &DensePolynomial<Fr>,
    global_parameters: &GlobalParameters,
) -> Vec<Proof> {
    unimplemented!()
}

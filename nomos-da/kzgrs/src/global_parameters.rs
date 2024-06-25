use super::GlobalParameters;
use ark_bls12_381::{fr::Fr, Bls12_381};
use ark_poly::polynomial::univariate::DensePolynomial;
use ark_poly_commit::kzg10::KZG10;
use rand::Rng;

pub fn global_parameters_from_randomness<R: Rng>(rng: &mut R) -> GlobalParameters {
    KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(8192, true, rng).unwrap()
}

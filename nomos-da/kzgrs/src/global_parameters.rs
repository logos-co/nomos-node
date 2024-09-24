// std
use std::{error::Error, fs::File};
// crates
use ark_bls12_381::{fr::Fr, Bls12_381};
use ark_poly::polynomial::univariate::DensePolynomial;
use ark_poly_commit::kzg10::{UniversalParams, KZG10};
use ark_serialize::{CanonicalDeserialize, Read};
use rand::Rng;
// internal
use super::GlobalParameters;

pub fn global_parameters_from_randomness<R: Rng>(rng: &mut R) -> GlobalParameters {
    KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(8192, true, rng).unwrap()
}

pub fn global_parameters_from_file(file_path: &str) -> Result<GlobalParameters, Box<dyn Error>> {
    let mut file = File::open(file_path)?;
    let mut serialized_data = Vec::new();
    file.read_to_end(&mut serialized_data)?;

    let params =
        UniversalParams::<Bls12_381>::deserialize_uncompressed_unchecked(&*serialized_data)?;
    Ok(params)
}

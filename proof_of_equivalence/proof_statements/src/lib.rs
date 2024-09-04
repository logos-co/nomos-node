use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ark_poly::univariate::DensePolynomial;
use ark_bls12_381::Fr;
use ark_ff::{PrimeField};
use ark_poly::{DenseUVPolynomial, Polynomial};

const BLOB_SIZE: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalencePublic {
    pub da_commitment: Vec<u8>,
    pub y_0: [u8; 32]
}

impl EquivalencePublic {
    pub fn new(
        da_commitment: Vec<u8>,
        y_0: [u8; 32]
    ) -> Self {
        Self {
            da_commitment,
            y_0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalencePrivate {
    pub coefficients: Vec<[u8; 32]>
}

impl EquivalencePrivate {
    pub fn new(
        coefficients: Vec<[u8; 32]>,
    ) -> Self {
        Self {
            coefficients,
        }
    }
    pub fn get_random_point(&self, da_commitment: Vec<u8>) -> [u8; 32] {
        assert_eq!(da_commitment.len(), 48);
        let mut hasher = Sha256::new();
        hasher.update(da_commitment);
        for i in 0..self.coefficients.len() {
            hasher.update(&self.coefficients[i]);
        }
        hasher.finalize().into()
    }

    pub fn evaluate(&self, point: [u8;32]) -> Fr {
        assert_eq!(self.coefficients.len(), BLOB_SIZE);
        // TODO: Optimize this part
        let mut bls_coefficients  = vec![];
        for i in 0..BLOB_SIZE {
            bls_coefficients.push(Fr::from_be_bytes_mod_order(&self.coefficients[i]));
        }
        let bls_point = Fr::from_be_bytes_mod_order(&point);
        let polynomial = DensePolynomial::from_coefficients_vec(bls_coefficients);
        polynomial.evaluate(&bls_point)
    }
}

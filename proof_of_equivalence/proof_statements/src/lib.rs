use serde::{Deserialize, Serialize};

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
        coefficients: Vec<Fr>,
    ) -> Self {
        Self {
            coefficients,
        }
    }
}

use serde::{Deserialize, Serialize};
use crypto_bigint::U256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalencePublic {
    pub da_commitment: Vec<u8>,
    pub y_0: U256
}

impl EquivalencePublic {
    pub fn new(
        da_commitment: Vec<u8>,
        y_0: U256
    ) -> Self {
        Self {
            da_commitment,
            y_0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalencePrivate {
    pub coefficients: Vec<U256>
}

impl EquivalencePrivate {
    pub fn new(
        coefficients: Vec<U256>,
    ) -> Self {
        Self {
            coefficients,
        }
    }
}

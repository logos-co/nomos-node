use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Groth16Proof {
    pub pi_a: Vec<u8>,
    pub pi_b: Vec<u8>,
    pub pi_c: Vec<u8>,
}

impl Groth16Proof {
    pub fn new(
        pi_a: Vec<u8>,
        pi_b: Vec<u8>,
        pi_c: Vec<u8>,
    ) -> Self {
        Self {
            pi_a,
            pi_b,
            pi_c,
        }
    }
}
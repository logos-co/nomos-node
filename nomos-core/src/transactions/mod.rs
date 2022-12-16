// std
use std::error::Error;
// crates
use serde::{Deserialize, Serialize};
// internal

#[derive(Serialize, Deserialize)]
pub enum Transaction {
    None,
}

impl Transaction {
    pub fn from_bytes(_bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        Ok(Self::None)
    }
}

use std::error::Error;

pub enum Transaction {
    None,
}

impl Transaction {
    pub fn from_bytes(_bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        Ok(Self::None)
    }
}

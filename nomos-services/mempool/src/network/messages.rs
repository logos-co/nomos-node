use nomos_core::transactions::Transaction;
use std::error::Error;

pub struct TransactionMsg {
    pub tx: Transaction,
    pub id: u64,
}

impl TransactionMsg {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let tx = Transaction::from_bytes(&bytes[8..])?;
        let id = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        Ok(Self { tx, id })
    }
}

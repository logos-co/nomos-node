use nomos_core::transactions::Transaction;
use std::error::Error;

pub struct TransactionMsg {
    pub tx: Transaction,
}

impl TransactionMsg {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let tx = Transaction::from_bytes(bytes)?;
        Ok(Self { tx })
    }
}

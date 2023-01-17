use crate::account::AccountId;
use crate::crypto::Signature;
use crate::wire::{self, deserializer};
use thiserror::Error;

/// Verified transactions
///
/// Can only be constructed if the signature is valid,
/// but does not imply that it can be successfully applied
/// to the ledger.
#[derive(Clone, Debug)]
pub struct Transaction {
    pub from: AccountId,
    pub to: AccountId,
    pub value: u64,
    // TODO: here for the moment because I still want to retain the ability
    // to go from `Transaction` to wire format. We could otherwise
    // save the id and rely on some storage
    _signature: Signature,
}

#[derive(Error, Debug)]
pub enum TransactionError {
    #[error(transparent)]
    InvalidStructure(#[from] wire::Error),
    #[error("Invalid Signature")]
    InvalidSignature,
}

impl Transaction {
    pub fn validate(data: &[u8]) -> Result<Transaction, TransactionError> {
        let mut deserializer = deserializer(data);
        let from = deserializer.deserialize::<AccountId>()?;
        let to = deserializer.deserialize::<AccountId>()?;
        let value = deserializer.deserialize::<u64>()?;
        let signature = deserializer.deserialize::<Signature>()?;

        // TODO: validate signature
        Ok(Transaction {
            from,
            to,
            value,
            _signature: signature,
        })
    }
}

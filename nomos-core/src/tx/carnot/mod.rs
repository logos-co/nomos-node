// std
// crates
use bytes::Bytes;
use serde::{Deserialize, Serialize};
// internal
pub use crate::tx::carnot::transaction::TransferTransaction;
use crate::tx::{Transaction, TransactionHasher};

mod transaction;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Tx {
    Transfer(TransferTransaction),
}

// TODO: We should probably abstract the de/serialization of the transaction as it s done in transaction.rs
fn hash_carnot_tx(tx: &Tx) -> [u8; 32] {
    use blake2::{
        digest::{consts::U32, Digest},
        Blake2b,
    };
    let mut hasher = Blake2b::<U32>::new();
    hasher.update(<Tx as Transaction>::as_bytes(tx));
    let res = hasher.finalize();
    res.into()
}

impl Transaction for Tx {
    const HASHER: TransactionHasher<Self> = hash_carnot_tx;
    type Hash = [u8; 32];

    fn as_bytes(&self) -> Bytes {
        [].to_vec().into()
    }
}

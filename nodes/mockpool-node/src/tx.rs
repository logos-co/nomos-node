use bytes::Bytes;
use nomos_core::tx::{Transaction, TransactionHasher};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
pub struct Tx(pub String);

fn hash_tx(tx: &Tx) -> String {
    tx.0.clone()
}

impl Transaction for Tx {
    const HASHER: TransactionHasher<Self> = hash_tx;
    type Hash = String;

    fn as_bytes(&self) -> Bytes {
        self.0.as_bytes().to_vec().into()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_txid() {
        let tx = Tx("test".to_string());
        let txid = TxId::from(&tx);
        assert_eq!(
            txid.0,
            [
                39, 227, 252, 176, 211, 134, 68, 39, 134, 158, 47, 7, 82, 40, 169, 232, 168, 118,
                240, 103, 84, 146, 127, 64, 60, 196, 126, 142, 172, 156, 124, 78
            ]
        );
    }
}

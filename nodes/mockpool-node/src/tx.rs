use blake2::digest::{Update, VariableOutput};
use blake2::Blake2bVar;
use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
pub struct Tx(pub String);

#[derive(Debug, Eq, Hash, PartialEq, Ord, Clone, PartialOrd)]
pub struct TxId([u8; 32]);

impl From<&Tx> for TxId {
    fn from(tx: &Tx) -> Self {
        let mut hasher = Blake2bVar::new(32).unwrap();
        hasher.update(
            bincode::serde::encode_to_vec(tx, bincode::config::standard())
                .unwrap()
                .as_slice(),
        );
        let mut id = [0u8; 32];
        hasher.finalize_variable(&mut id).unwrap();
        Self(id)
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

use blake2::{Blake2s256, Digest};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
pub struct Tx(pub String);

#[derive(Debug, Eq, Hash, PartialEq, Ord, Clone, PartialOrd)]
pub struct TxId([u8; 32]);

impl From<&Tx> for TxId {
    fn from(tx: &Tx) -> Self {
        let mut hasher = Blake2s256::new();
        hasher.update(bincode::serde::encode_to_vec(tx, bincode::config::standard()).unwrap());
        let mut id = [0u8; 32];
        id.copy_from_slice(hasher.finalize().as_slice());
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
                26, 113, 217, 35, 178, 18, 65, 215, 30, 117, 195, 9, 189, 146, 124, 78, 66, 91, 97,
                21, 254, 51, 156, 155, 144, 52, 135, 125, 51, 128, 186, 244
            ]
        );
    }
}

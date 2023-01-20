use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tx(nomos_core::tx::Tx);

impl Hash for Tx {
    fn hash<H: Hasher>(&self, _state: &mut H) {
        todo!("why da fuck do we need this?")
    }
}

#[derive(Debug, Eq, Hash, PartialEq, Ord, Clone, PartialOrd)]
pub struct TxId([u8; 32]);

impl From<Tx> for TxId {
    fn from(tx: Tx) -> Self {
        let mut hasher = Blake2b512::new();
        hasher.update(bincode::serde::encode_to_vec(tx, bincode::config::standard()).unwrap());
        let mut id = [0u8; 32];
        id.copy_from_slice(hasher.finalize().as_slice());
        Self(id)
    }
}

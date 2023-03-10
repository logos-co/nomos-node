// std
// crates
use serde::{Deserialize, Serialize};
// internal
pub use crate::tx::transaction::Transaction;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Tx {
    Transfer(Transaction),
}

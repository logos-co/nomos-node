// std
// crates
use serde::{Deserialize, Serialize};
// internal

#[derive(Serialize, Deserialize)]
pub struct TransactionMsg<Tx> {
    pub tx: Tx,
}

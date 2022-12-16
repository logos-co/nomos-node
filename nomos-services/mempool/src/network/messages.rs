// std
// crates
use serde::{Deserialize, Serialize};
// internal

#[derive(Serialize, Deserialize)]
pub struct TransactionMsg<Tx, Id> {
    pub tx: Tx,
    pub id: Id,
}

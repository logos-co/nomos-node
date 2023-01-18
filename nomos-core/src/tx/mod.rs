mod transaction;
use serde::{Deserialize, Serialize};
pub use transaction::Transaction;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Tx {
    Transfer(Transaction),
}

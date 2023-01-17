mod transaction;
pub use transaction::Transaction;

#[derive(Clone, Debug)]
pub enum Tx {
    Transfer(Transaction),
}

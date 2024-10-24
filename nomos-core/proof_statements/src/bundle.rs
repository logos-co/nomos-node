use cl::{Balance, BalanceWitness};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePublic {
    pub balances: Vec<Balance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePrivate {
    pub balances: Vec<BalanceWitness>,
}

// std
// crates
use serde::{Deserialize, Serialize};
// internal

#[derive(Serialize, Deserialize)]
pub struct TransactionMsg<Tx> {
    pub tx: Tx,
}

#[cfg(feature = "mock")]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MockTransactionMsg {
    pub msg: nomos_network::backends::mock::MockMessage,
}

#[cfg(feature = "mock")]
impl From<&MockTransactionMsg> for String {
    fn from(msg: &MockTransactionMsg) -> Self {
        msg.msg.payload()
    }
}

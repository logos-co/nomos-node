use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize)]
pub enum MockTransactionMsg {
    Request(nomos_network::backends::mock::MockMessage),
    Response(nomos_network::backends::mock::MockMessage),
}

#[derive(Debug, Eq, Hash, PartialEq, Ord, Clone, PartialOrd)]
pub struct MockTxId([u8; 32]);

impl From<&MockTransactionMsg> for MockTxId {
    fn from(tx: &MockTransactionMsg) -> Self {
        let mut hasher = Blake2bVar::new(32).unwrap();
        hasher.update(serde_json::to_string(tx).unwrap().as_bytes());
        let mut id = [0u8; 32];
        hasher.finalize_variable(&mut id).unwrap();
        Self(id)
    }
}

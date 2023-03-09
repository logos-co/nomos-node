use crate::wire::serialize;
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use nomos_network::backends::mock::MockMessage;

#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MockTransaction {
    id: MockTxId,
    content: MockMessage,
}

impl MockTransaction {
    pub fn new(content: MockMessage) -> Self {
        let id = MockTxId::from(&content);
        Self { id, content }
    }

    pub fn message(&self) -> &MockMessage {
        &self.content
    }
}

impl From<&nomos_network::backends::mock::MockMessage> for MockTransaction {
    fn from(msg: &nomos_network::backends::mock::MockMessage) -> Self {
        let id = MockTxId::from(msg);
        Self {
            id,
            content: msg.clone(),
        }
    }
}

#[derive(
    Debug, Eq, Hash, PartialEq, Ord, Copy, Clone, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub struct MockTxId([u8; 32]);

impl From<[u8; 32]> for MockTxId {
    fn from(tx_id: [u8; 32]) -> Self {
        Self(tx_id)
    }
}

impl core::ops::Deref for MockTxId {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for MockTxId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl MockTxId {
    pub fn new(tx_id: [u8; 32]) -> MockTxId {
        MockTxId(tx_id)
    }
}

impl From<&nomos_network::backends::mock::MockMessage> for MockTxId {
    fn from(msg: &nomos_network::backends::mock::MockMessage) -> Self {
        let mut hasher = Blake2bVar::new(32).unwrap();
        hasher.update(&serialize(msg).unwrap());
        let mut id = [0u8; 32];
        hasher.finalize_variable(&mut id).unwrap();
        Self(id)
    }
}

impl From<&MockTransaction> for MockTxId {
    fn from(msg: &MockTransaction) -> Self {
        msg.id
    }
}

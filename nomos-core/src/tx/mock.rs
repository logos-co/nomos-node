use crate::tx::{Transaction, TransactionHasher};
use crate::wire;
use crate::wire::serialize;
use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};
use bytes::{Bytes, BytesMut};
use serde::Serialize;

#[derive(Debug, Clone, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub struct MockTransaction<M> {
    id: MockTxId,
    content: M,
}

impl<M: Serialize> MockTransaction<M> {
    pub fn new(content: M) -> Self {
        let id = MockTxId::from(serialize(&content).unwrap().as_slice());
        Self { id, content }
    }

    pub fn message(&self) -> &M {
        &self.content
    }

    pub fn id(&self) -> MockTxId {
        self.id
    }

    fn as_bytes(&self) -> Bytes {
        let mut buff = BytesMut::new();
        wire::serializer_into_buffer(&mut buff)
            .serialize_into(&self)
            .expect("MockTransaction serialization to buffer failed");
        buff.freeze()
    }
}

impl<M: Serialize> Transaction for MockTransaction<M> {
    const HASHER: TransactionHasher<Self> = MockTransaction::id;
    type Hash = MockTxId;

    fn as_bytes(&self) -> Bytes {
        MockTransaction::as_bytes(self)
    }
}

impl<M: Serialize> From<M> for MockTransaction<M> {
    fn from(msg: M) -> Self {
        let id = MockTxId::from(serialize(&msg).unwrap().as_slice());
        Self { id, content: msg }
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

impl From<&[u8]> for MockTxId {
    fn from(msg: &[u8]) -> Self {
        let mut hasher = Blake2bVar::new(32).unwrap();
        hasher.update(msg);
        let mut id = [0u8; 32];
        hasher.finalize_variable(&mut id).unwrap();
        Self(id)
    }
}

impl<M> From<&MockTransaction<M>> for MockTxId {
    fn from(msg: &MockTransaction<M>) -> Self {
        msg.id
    }
}

#[derive(Default)]
pub struct MockTxVerifier;

impl MockTxVerifier {
    pub fn verify_tx<Tx>(&self, _tx: &Tx) -> bool
    where
        Tx: Transaction,
    {
        true
    }
}

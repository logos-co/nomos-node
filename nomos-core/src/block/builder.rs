// std
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::hash::Hash;
use std::marker::PhantomData;
// crates

// internal
use crate::block::Block;
use crate::da::blob::{Blob, BlobSelect};
use crate::tx::{Transaction, TxSelect};
use consensus_engine::overlay::RandomBeaconState;
use consensus_engine::{NodeId, Qc, View};

pub struct BlockBuilder<Tx, Blob, TxSelector, BlobSelector> {
    _tx: PhantomData<Tx>,
    _blob: PhantomData<Blob>,
    tx_selector: TxSelector,
    blob_selector: BlobSelector,
    view: Option<View>,
    parent_qc: Option<Qc>,
    proposer: Option<NodeId>,
    beacon: Option<RandomBeaconState>,
    txs: Option<Box<dyn Iterator<Item = Tx>>>,
    blobs: Option<Box<dyn Iterator<Item = Blob>>>,
}

impl<Tx, B, TxSelector, BlobSelector> BlockBuilder<Tx, B, TxSelector, BlobSelector>
where
    Tx: Transaction + Clone + Eq + Hash + Serialize + DeserializeOwned,
    B: Blob + Clone + Eq + Hash + Serialize + DeserializeOwned,
    TxSelector: TxSelect<Tx = Tx>,
    BlobSelector: BlobSelect<Blob = B>,
{
    pub fn new(tx_selector: TxSelector, blob_selector: BlobSelector) -> Self {
        Self {
            _tx: Default::default(),
            _blob: Default::default(),
            tx_selector,
            blob_selector,
            view: None,
            parent_qc: None,
            proposer: None,
            beacon: None,
            txs: None,
            blobs: None,
        }
    }

    #[must_use]
    pub fn with_view(mut self, view: View) -> Self {
        self.view = Some(view);
        self
    }

    #[must_use]
    pub fn with_parent_qc(mut self, qc: Qc) -> Self {
        self.parent_qc = Some(qc);
        self
    }

    #[must_use]
    pub fn with_proposer(mut self, proposer: NodeId) -> Self {
        self.proposer = Some(proposer);
        self
    }

    #[must_use]
    pub fn with_beacon_state(mut self, beacon: RandomBeaconState) -> Self {
        self.beacon = Some(beacon);
        self
    }

    #[allow(clippy::result_large_err)]
    pub fn build(self) -> Result<Block<Tx, B>, Self> {
        if let Self {
            tx_selector,
            blob_selector,
            view: Some(view),
            parent_qc: Some(parent_qc),
            proposer: Some(proposer),
            beacon: Some(beacon),
            txs: Some(txs),
            blobs: Some(blobs),
            ..
        } = self
        {
            Ok(Block::new(
                view,
                parent_qc,
                tx_selector.select_tx_from(txs),
                blob_selector.select_blob_from(blobs),
                proposer,
                beacon,
            ))
        } else {
            Err(self)
        }
    }
}

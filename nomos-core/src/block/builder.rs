// std
use std::hash::Hash;
// crates
use serde::de::DeserializeOwned;
use serde::Serialize;
// internal
use crate::block::Block;
use crate::da::blob::{Blob, BlobSelect};
use crate::tx::{Transaction, TxSelect};
use consensus_engine::overlay::RandomBeaconState;
use consensus_engine::{NodeId, Qc, View};

/// Wrapper over a block building `new` method than holds intermediary state and can be
/// passed around. It also compounds the transaction selection and blob selection heuristics to be
/// used for transaction and blob selection.
///
/// Example:
/// ``` ignore
/// use nomos_core::block::builder::BlockBuilder;
/// let builder: BlockBuilder<(), (), FirstTx, FirstBlob> = {
///     BlockBuilder::new( FirstTx::default(), FirstBlob::default())
///         .with_view(View::from(0))
///         .with_parent_qc(qc)
///         .with_proposer(proposer)
///         .with_beacon_state(beacon)
///         .with_transactions([tx1].into_iter())
///         .with_blobs([blob1].into_iter())
/// };
/// builder.build().expect("All block attributes should have been set")
/// ```
pub struct BlockBuilder<Tx, Blob, TxSelector, BlobSelector> {
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
    pub fn new(
        tx_selector_settings: TxSelector::Settings,
        blob_selector_settings: BlobSelector::Settings,
    ) -> Self {
        Self {
            tx_selector: TxSelector::new(tx_selector_settings),
            blob_selector: BlobSelector::new(blob_selector_settings),
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

    #[must_use]
    pub fn with_transactions(mut self, txs: impl Iterator<Item = Tx> + 'static) -> Self {
        self.txs = Some(Box::new(txs));
        self
    }

    #[must_use]
    pub fn with_blobs(mut self, blobs: impl Iterator<Item = B> + 'static) -> Self {
        self.blobs = Some(Box::new(blobs));
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

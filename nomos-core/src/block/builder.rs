// std
use indexmap::IndexSet;
use std::hash::Hash;
// crates
use serde::de::DeserializeOwned;
use serde::Serialize;
// internal
use crate::block::Block;
use crate::crypto::Blake2b;
use crate::da::blob::{info::DispersedBlobInfo, BlobSelect};
use crate::header::{
    carnot::Builder as CarnotBuilder, cryptarchia::Builder as CryptarchiaBuilder, Header, HeaderId,
};
use crate::tx::{Transaction, TxSelect};
use crate::wire;
use blake2::digest::Digest;
use carnot_engine::overlay::RandomBeaconState;
use carnot_engine::{LeaderProof, Qc, View};
/// Wrapper over a block building `new` method than holds intermediary state and can be
/// passed around. It also compounds the transaction selection and blob selection heuristics to be
/// used for transaction and blob selection.
///
/// Example:
/// ``` ignore
/// use nomos_core::block::builder::BlockBuilder;
/// let builder: BlockBuilder<(), (), FirstTx, FirstBlob> = {
///     BlockBuilder::new( FirstTx::default(), FirstBlob::default())
///         .with_transactions([tx1].into_iter())
///         .with_blobs([blob1].into_iter())
/// };
/// builder.build().expect("All block attributes should have been set")
/// ```
pub struct BlockBuilder<Tx, Blob, TxSelector, BlobSelector> {
    tx_selector: TxSelector,
    blob_selector: BlobSelector,
    carnot_header_builder: Option<CarnotBuilder>,
    cryptarchia_header_builder: Option<CryptarchiaBuilder>,
    txs: Option<Box<dyn Iterator<Item = Tx>>>,
    blobs: Option<Box<dyn Iterator<Item = Blob>>>,
}

impl<Tx, C, TxSelector, BlobSelector> BlockBuilder<Tx, C, TxSelector, BlobSelector>
where
    Tx: Clone + Eq + Hash,
    C: Clone + Eq + Hash,
{
    pub fn empty_carnot(
        beacon: RandomBeaconState,
        view: View,
        parent_qc: Qc<HeaderId>,
        leader_proof: LeaderProof,
    ) -> Block<Tx, C> {
        Block {
            header: Header::Carnot(
                CarnotBuilder::new(beacon, view, parent_qc, leader_proof).build([0; 32].into(), 0),
            ),
            cl_transactions: IndexSet::new(),
            bl_blobs: IndexSet::new(),
        }
    }
}

impl<Tx, B, TxSelector, BlobSelector> BlockBuilder<Tx, B, TxSelector, BlobSelector>
where
    Tx: Transaction + Clone + Eq + Hash + Serialize + DeserializeOwned,
    B: DispersedBlobInfo + Clone + Eq + Hash + Serialize + DeserializeOwned,
    TxSelector: TxSelect<Tx = Tx>,
    BlobSelector: BlobSelect<BlobId = B>,
{
    pub fn new(tx_selector: TxSelector, blob_selector: BlobSelector) -> Self {
        Self {
            tx_selector,
            blob_selector,
            carnot_header_builder: None,
            cryptarchia_header_builder: None,
            txs: None,
            blobs: None,
        }
    }

    #[must_use]
    pub fn with_carnot_builder(mut self, carnot_header_builder: CarnotBuilder) -> Self {
        self.carnot_header_builder = Some(carnot_header_builder);
        self
    }

    #[must_use]
    pub fn with_cryptarchia_builder(
        mut self,
        cryptarchia_header_builder: CryptarchiaBuilder,
    ) -> Self {
        self.cryptarchia_header_builder = Some(cryptarchia_header_builder);
        self
    }

    #[must_use]
    pub fn with_transactions(mut self, txs: impl Iterator<Item = Tx> + 'static) -> Self {
        self.txs = Some(Box::new(txs));
        self
    }

    #[must_use]
    pub fn with_blobs_certificates(
        mut self,
        blobs_certificates: impl Iterator<Item = B> + 'static,
    ) -> Self {
        self.blobs = Some(Box::new(blobs_certificates));
        self
    }

    #[allow(clippy::result_large_err)]
    pub fn build(self) -> Result<Block<Tx, B>, String> {
        if let Self {
            tx_selector,
            blob_selector,
            carnot_header_builder: carnot_builder,
            cryptarchia_header_builder: cryptarchia_builder,
            txs: Some(txs),
            blobs: Some(blobs),
        } = self
        {
            let txs = tx_selector.select_tx_from(txs).collect::<IndexSet<_>>();
            let blobs = blob_selector
                .select_blob_from(blobs)
                .collect::<IndexSet<_>>();

            let serialized_content = wire::serialize(&(&txs, &blobs)).unwrap();
            let content_size = u32::try_from(serialized_content.len()).map_err(|_| {
                format!(
                    "Content is too big: {} out of {} max",
                    serialized_content.len(),
                    u32::MAX
                )
            })?;
            let content_id = <[u8; 32]>::from(Blake2b::digest(&serialized_content)).into();

            let header = match (carnot_builder, cryptarchia_builder) {
                (Some(carnot_builder), None) => {
                    Header::Carnot(carnot_builder.build(content_id, content_size))
                }
                (None, Some(cryptarchia_builder)) => {
                    Header::Cryptarchia(cryptarchia_builder.build(content_id, content_size))
                }
                _ => return Err("Exactly one header builder should be set".to_string()),
            };

            Ok(Block {
                header,
                cl_transactions: txs,
                bl_blobs: blobs,
            })
        } else {
            Err("incomplete block".to_string())
        }
    }
}

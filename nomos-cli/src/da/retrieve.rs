use full_replication::Blob;
use nomos_core::{block::Block, da::certificate::Certificate};
use nomos_node::Tx;
use reqwest::Url;
use thiserror::Error;

use crate::api::da::get_blobs;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Block not found")]
    NotFound,
}

/// Return the blobs whose certificate has been included in the provided block.
pub async fn get_block_blobs(
    node: &Url,
    block: &Block<Tx, full_replication::Certificate>,
) -> Result<Vec<Blob>, Error> {
    Ok(get_blobs(node, block.blobs().map(|cert| cert.blob()).collect()).await?)
}

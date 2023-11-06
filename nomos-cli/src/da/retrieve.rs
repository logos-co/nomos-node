use consensus_engine::BlockId;
use full_replication::Blob;
use nomos_core::da::certificate::Certificate;
use reqwest::Url;
use thiserror::Error;

use crate::api::{da::get_blobs, storage::get_block_contents};

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Block not found")]
    NotFound,
}

/// Return the blobs whose certificate has been included in the provided block.
pub async fn get_block_blobs(node: &Url, block: &BlockId) -> Result<Vec<Blob>, Error> {
    let block = get_block_contents(node, block)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(get_blobs(node, block.blobs().map(|cert| cert.blob()).collect()).await?)
}

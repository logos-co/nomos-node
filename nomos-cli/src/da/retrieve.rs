use full_replication::Blob;
use nomos_core::header::HeaderId;
use reqwest::Url;
use thiserror::Error;

use crate::api::storage::get_block_contents;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Block not found")]
    NotFound,
}

/// Return the blobs whose certificate has been included in the provided block.
pub async fn get_block_blobs(node: &Url, block: &HeaderId) -> Result<Vec<Blob>, Error> {
    let block = get_block_contents(node, block)
        .await?
        .ok_or(Error::NotFound)?;

    let blobs = block.blobs().map(|cert| cert.blob()).collect::<Vec<_>>();

    if blobs.is_empty() {
        return Ok(vec![]);
    }

    let n_blobs = blobs.len();
    let resp = get_blobs(node, blobs).await?;
    if resp.len() != n_blobs {
        tracing::warn!("Only {}/{} blobs returned", resp.len(), n_blobs);
    }
    Ok(resp)
}

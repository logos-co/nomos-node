use consensus_engine::BlockId;
use full_replication::{Blob, Certificate};
use nomos_core::{
    block::Block,
    da::{blob, certificate::Certificate as _},
};
use nomos_node::Tx;
use reqwest::Url;
use thiserror::Error;

const BLOCK_PATH: &str = "storage/block";
const BLOBS_PATH: &str = "da/blobs";

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Block not found")]
    NotFound,
}

/// Return the blobs whose certificate has been included in the proived block.
pub async fn get_block_blobs(node: Url, block: BlockId) -> Result<Vec<Blob>, Error> {
    let block = get_block_contents(&node, block)
        .await?
        .ok_or(Error::NotFound)?;

    Ok(get_blobs(node, block.blobs().map(|cert| cert.blob()).collect()).await?)
}

pub async fn get_block_contents(
    node: &Url,
    block: BlockId,
) -> Result<Option<Block<Tx, Certificate>>, reqwest::Error> {
    let block = reqwest::Client::new()
        .post(node.join(BLOCK_PATH).unwrap())
        .body(serde_json::to_string(&block).unwrap())
        .send()
        .await?
        .json()
        .await?;
    Ok(block)
}

pub async fn get_blobs(
    node: Url,
    ids: Vec<<Blob as blob::Blob>::Hash>,
) -> Result<Vec<Blob>, reqwest::Error> {
    reqwest::Client::new()
        .post(node.join(BLOBS_PATH).unwrap())
        .body(serde_json::to_string(&ids).unwrap())
        .send()
        .await?
        .json()
        .await
}

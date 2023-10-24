use consensus_engine::BlockId;
use full_replication::{Blob, Certificate};
use nomos_core::block::Block;
use nomos_node::Tx;
use reqwest::Url;

/// Return the blobs whose certificate has been included in the proived block.
pub async fn get_block_blobs(
    node: Url,
    block: BlockId,
) -> Result<Vec<Blob>, Box<dyn std::error::Error>> {
    let block = get_block_contents(node, block)
        .await?
        .ok_or(Box::new("Block not found"))?;

    let mut res = Vec::new();
    for cert in block.blobs() {
        res.push(value)
    }
}

pub async fn get_block_contents(
    node: Url,
    block: BlockId,
) -> Result<Option<Block<Tx, Certificate>>, Box<dyn std::error::Error>> {
    let block = node.join("storage/block")?;
    let block = reqwest::get(block).await?.bytes().await?;
    let block = serde_json::deserialize(&block)?;
    Ok(block)
}

pub async fn get_blobs(
    node: Url,
    ids: Vec<Blob::Hash>,
) -> Result<Vec<Blob>, Box<dyn std::error::Error>> {
    let mut res = Vec::new();
    for id in ids {
        let blob = get_blob(node, id).await?;
        res.push(blob);
    }
    Ok(res)
}

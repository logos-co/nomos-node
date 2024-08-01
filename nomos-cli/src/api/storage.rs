use super::CLIENT;
use full_replication::BlobInfo;
use nomos_core::block::Block;
use nomos_core::header::HeaderId;
use nomos_node::Tx;
use reqwest::Url;

pub async fn get_block_contents(
    node: &Url,
    block: &HeaderId,
) -> Result<Option<Block<Tx, BlobInfo>>, reqwest::Error> {
    const BLOCK_PATH: &str = "storage/block";
    CLIENT
        .post(node.join(BLOCK_PATH).unwrap())
        .json(block)
        .send()
        .await?
        .json()
        .await
}

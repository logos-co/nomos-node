use super::CLIENT;
use consensus_engine::BlockId;
use full_replication::Certificate;
use nomos_core::block::Block;
use nomos_node::Tx;
use reqwest::Url;

pub async fn get_block_contents(
    node: &Url,
    block: &BlockId,
) -> Result<Option<Block<Tx, Certificate>>, reqwest::Error> {
    const BLOCK_PATH: &str = "storage/block";
    let block = CLIENT
        .post(node.join(BLOCK_PATH).unwrap())
        .body(serde_json::to_string(block).unwrap())
        .send()
        .await?
        .json()
        .await?;
    Ok(block)
}

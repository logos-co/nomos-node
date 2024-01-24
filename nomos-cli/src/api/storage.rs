use super::CLIENT;
use carnot_engine::BlockId;
use full_replication::Certificate;
use nomos_core::block::Block;
use nomos_node::Tx;
use reqwest::Url;

pub async fn get_block_contents(
    node: &Url,
    block: &BlockId,
) -> Result<Option<Block<Tx, Certificate>>, reqwest::Error> {
    const BLOCK_PATH: &str = "storage/block";
    CLIENT
        .post(node.join(BLOCK_PATH).unwrap())
        .json(block)
        .send()
        .await?
        .json()
        .await
}

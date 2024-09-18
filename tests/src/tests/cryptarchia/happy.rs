use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use std::time::Duration;
use tests::{adjust_timeout, nodes::NomosNode, Node, SpawnConfig};

// how many times more than the expected time to produce a predefined number of blocks we wait before timing out
const TIMEOUT_MULTIPLIER: f64 = 3.0;
// how long we let the chain grow before checking the block at tip - k is the same in all chains
const CHAIN_LENGTH_MULTIPLIER: u32 = 2;

async fn happy_test(nodes: &[NomosNode]) {
    let config = nodes[0].config();
    let security_param = config.cryptarchia.config.consensus_config.security_param;
    let n_blocks = security_param * CHAIN_LENGTH_MULTIPLIER;
    println!("waiting for {n_blocks} blocks");
    let timeout = (n_blocks as f64 / config.cryptarchia.config.consensus_config.active_slot_coeff
        * config.cryptarchia.time.slot_duration.as_secs() as f64
        * TIMEOUT_MULTIPLIER)
        .floor() as u64;
    let timeout = adjust_timeout(Duration::from_secs(timeout));
    let timeout = tokio::time::sleep(timeout);
    tokio::select! {
        _ = timeout => panic!("timed out waiting for nodes to produce {} blocks", n_blocks),
        _ = async { while stream::iter(nodes)
            .any(|n| async move { (n.consensus_info().await.height as u32) < n_blocks })
            .await
        {
            println!(
                "waiting... {}",
                stream::iter(nodes)
                    .then(|n| async move { format!("{}", n.consensus_info().await.height) })
                    .collect::<Vec<_>>()
                    .await
                    .join(" | ")
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        } => {}
    };

    let last_committed_block_height = n_blocks - security_param;
    println!("{:?}", nodes[0].consensus_info().await);

    let infos = stream::iter(nodes)
        .then(|n| async move { n.get_headers(None, None).await })
        .map(|blocks| blocks[last_committed_block_height as usize])
        .collect::<HashSet<_>>()
        .await;

    assert_eq!(infos.len(), 1, "consensus not reached");
}

#[tokio::test]
async fn two_nodes_happy() {
    let nodes = NomosNode::spawn_nodes(SpawnConfig::star_happy(2, Default::default())).await;
    happy_test(&nodes).await;
}

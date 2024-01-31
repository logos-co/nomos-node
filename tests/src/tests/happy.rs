use carnot_engine::{Qc, View};
use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use std::time::Duration;
use tests::{adjust_timeout, Node, NomosNode, SpawnConfig};

const TARGET_VIEW: View = View::new(20);

#[derive(serde::Serialize)]
struct Info {
    node_id: String,
    block_id: String,
    view: View,
}

async fn happy_test(nodes: &[NomosNode]) {
    let timeout = adjust_timeout(Duration::from_secs(30));
    let timeout = tokio::time::sleep(timeout);
    tokio::select! {
        _ = timeout => panic!("timed out waiting for nodes to reach view {}", TARGET_VIEW),
        _ = async { while stream::iter(nodes)
            .any(|n| async move { n.consensus_info().await.current_view < TARGET_VIEW })
            .await
        {
            println!(
                "waiting... {}",
                stream::iter(nodes)
                    .then(|n| async move { format!("{}", n.consensus_info().await.current_view) })
                    .collect::<Vec<_>>()
                    .await
                    .join(" | ")
            );
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        } => {}
    };

    let infos = stream::iter(nodes)
        .then(|n| async move { n.get_blocks_info(None, None).await })
        .collect::<Vec<_>>()
        .await;
    // check that they have the same block
    let blocks = infos
        .iter()
        .map(|i| i.iter().find(|b| b.view == TARGET_VIEW).unwrap())
        .collect::<HashSet<_>>();

    // try to see if we have invalid blocks
    let invalid_blocks = infos
        .iter()
        .zip(nodes.iter())
        .flat_map(|(blocks, node)| {
            blocks.iter().filter_map(|b| match &b.parent_qc {
                Qc::Standard(_) => None,
                Qc::Aggregated(_) => Some(Info {
                    node_id: node.id().to_string(),
                    block_id: b.id.to_string(),
                    view: b.view,
                }),
            })
        })
        .collect::<Vec<_>>();

    assert!(
        invalid_blocks.is_empty(),
        "{}",
        serde_json::to_string_pretty(&invalid_blocks).unwrap()
    );
    assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn two_nodes_happy() {
    let nodes = NomosNode::spawn_nodes(SpawnConfig::chain_happy(2)).await;
    happy_test(&nodes).await;
}

#[tokio::test]
async fn ten_nodes_happy() {
    let nodes = NomosNode::spawn_nodes(SpawnConfig::chain_happy(10)).await;
    happy_test(&nodes).await;
}

#[tokio::test]
async fn test_get_block() {
    let nodes = NomosNode::spawn_nodes(SpawnConfig::chain_happy(2)).await;
    happy_test(&nodes).await;
    let id = nodes[0].consensus_info().await.last_committed_block.id;
    tokio::time::timeout(Duration::from_secs(10), async {
        while nodes[0].get_block(id).await.is_none() {
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("trying...");
        }
    })
    .await
    .unwrap();
}

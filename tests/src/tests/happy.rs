use consensus_engine::View;
use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use tests::{Node, NomosNode, SpawnConfig};

const TARGET_VIEW: View = View::new(20);

async fn happy_test(nodes: Vec<NomosNode>) {
    let timeout = std::time::Duration::from_secs(20);
    let timeout = tokio::time::sleep(timeout);
    tokio::select! {
        _ = timeout => panic!("timed out waiting for nodes to reach view {}", TARGET_VIEW),
        _ = async { while stream::iter(&nodes)
            .any(|n| async move { n.consensus_info().await.current_view < TARGET_VIEW })
            .await
        {
            println!(
                "waiting... {}",
                stream::iter(&nodes)
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
        .then(|n| async move { n.consensus_info().await })
        .collect::<Vec<_>>()
        .await;
    // check that they have the same block
    let blocks = infos
        .iter()
        .map(|i| {
            i.safe_blocks
                .values()
                .find(|b| b.view == TARGET_VIEW)
                .unwrap()
        })
        .collect::<HashSet<_>>();
    assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn two_nodes_happy() {
    let nodes = NomosNode::spawn_nodes(SpawnConfig::Star { n_participants: 2 }).await;
    happy_test(nodes).await;
}

#[tokio::test]
async fn ten_nodes_happy() {
    let nodes = NomosNode::spawn_nodes(SpawnConfig::Star { n_participants: 10 }).await;
    happy_test(nodes).await;
}

use consensus_engine::{Qc, View};
use fraction::{Fraction, One};
use futures::stream::{self, StreamExt};
use std::collections::HashSet;
use std::time::Duration;
use tests::{MixNode, Node, NomosNode, SpawnConfig};

const TARGET_VIEW: View = View::new(20);

#[derive(serde::Serialize)]
struct Info {
    id: String,
    view: View,
}

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

    // try to see if we have invalid blocks
    let invalid_blocks = infos
        .iter()
        .flat_map(|i| {
            i.safe_blocks.values().filter_map(|b| match &b.parent_qc {
                Qc::Standard(_) => None,
                Qc::Aggregated(_) => Some(Info {
                    id: b.id.to_string(),
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
    let (_mixnodes, mixnet_node_configs, mixnet_topology) = MixNode::spawn_nodes(2).await;
    let nodes = NomosNode::spawn_nodes(SpawnConfig::Star {
        n_participants: 2,
        threshold: Fraction::one(),
        timeout: Duration::from_secs(10),
        mixnet_node_configs,
        mixnet_topology,
    })
    .await;
    happy_test(nodes).await;
}

#[tokio::test]
async fn ten_nodes_happy() {
    let (_mixnodes, mixnet_node_configs, mixnet_topology) = MixNode::spawn_nodes(3).await;
    let nodes = NomosNode::spawn_nodes(SpawnConfig::Star {
        n_participants: 10,
        threshold: Fraction::one(),
        timeout: Duration::from_secs(10),
        mixnet_node_configs,
        mixnet_topology,
    })
    .await;
    happy_test(nodes).await;
}

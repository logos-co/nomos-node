use consensus_engine::{AggregateQc, Block, Qc, View};
use fraction::{Fraction, One};
use futures::stream::{self, StreamExt};
use std::fs::OpenOptions;
use std::time::Duration;
use std::{collections::HashSet, fs::File};
use tests::{MixNode, Node, NomosNode, SpawnConfig};

const TARGET_VIEW: View = View::new(20);

pub fn persist(name: &'static str, blocks: impl Iterator<Item = &Block>) {
    // We need to persist the block to disk before dropping it
    // if there is a aggregated qc when testing happy path
    if let Qc::Aggregated(qcs) = &block.parent_qc {
        #[derive(serde::Serialize)]
        struct Info<'a> {
            id: String,
            view: View,
            qcs: &'a AggregateQc,
        }

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(format!("{name}.json"))
            .unwrap();
        let infos = blocks
            .map(|b| Info {
                id: format!("{}", header.id),
                view: header.view,
                qcs,
            })
            .collect::<Vec<_>>();
        // Use pretty print to make it easier to read, because we need the this for debugging.
        serde_json::to_writer_pretty(&mut *file, &infos).unwrap();
    }
}

async fn happy_test(name: &'static str, nodes: Vec<NomosNode>) {
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

    // persist the blocks for debugging
    persist(name, infos.iter().flat_map(|i| i.safe_blocks.values()));
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
    happy_test("two_nodes", nodes).await;
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
    happy_test("ten_nodes", nodes).await;
}

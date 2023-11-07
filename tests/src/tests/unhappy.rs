use consensus_engine::{Block, NodeId, TimeoutQc, View};
use fraction::Fraction;
use futures::stream::{self, StreamExt};
use nomos_consensus::CarnotInfo;
use std::collections::HashSet;
use tests::{ConsensusConfig, MixNode, Node, NomosNode, SpawnConfig};

const TARGET_VIEW: View = View::new(20);
const DUMMY_NODE_ID: NodeId = NodeId::new([0u8; 32]);

#[tokio::test]
async fn ten_nodes_one_down() {
    let (_mixnodes, mixnet_config) = MixNode::spawn_nodes(3).await;
    let mut nodes = NomosNode::spawn_nodes(SpawnConfig::Chain {
        consensus: ConsensusConfig {
            n_participants: 10,
            threshold: Fraction::new(9u32, 10u32),
            timeout: std::time::Duration::from_secs(5),
        },
        mixnet: mixnet_config,
    })
    .await;
    let mut failed_node = nodes.pop().unwrap();
    failed_node.stop();
    let timeout = std::time::Duration::from_secs(120);
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

    let (infos, blocks): (Vec<_>, Vec<_>) = stream::iter(nodes)
        .then(|n| async move {
            (
                n.consensus_info().await,
                n.get_blocks_info(None, None).await,
            )
        })
        .unzip()
        .await;

    let target_block = assert_block_consensus(&blocks, TARGET_VIEW);

    // If no node has the target block, check that TARGET_VIEW was reached by timeout_qc.
    if target_block.is_none() {
        println!("No node has the block with {TARGET_VIEW:?}. Checking timeout_qcs...");
        assert_timeout_qc_consensus(&infos, TARGET_VIEW.prev());
    }
}

// Check if all nodes have the same block at the specific view.
fn assert_block_consensus<'a>(blocks: &[Vec<Block>], view: View) -> Option<Block> {
    let blocks = blocks
        .into_iter()
        .map(|b| b.iter().find(|b| b.view == view))
        .collect::<HashSet<_>>();
    // Every nodes must have the same target block (Some(block))
    // , or no node must have it (None).
    assert_eq!(
        blocks.len(),
        1,
        "multiple blocks found at {:?}: {:?}",
        view,
        blocks
    );

    blocks.iter().next().unwrap().cloned()
}

// Check if all nodes have the same timeout_qc at the specific view.
fn assert_timeout_qc_consensus<'a>(
    consensus_infos: impl IntoIterator<Item = &'a CarnotInfo>,
    view: View,
) -> TimeoutQc {
    let timeout_qcs = consensus_infos
        .into_iter()
        .map(|i| {
            i.last_view_timeout_qc.clone().map(|timeout_qc| {
                // Masking the `sender` field because we want timeout_qcs from different
                // senders to be considered the same if all other fields are the same.
                TimeoutQc::new(
                    timeout_qc.view(),
                    timeout_qc.high_qc().clone(),
                    DUMMY_NODE_ID,
                )
            })
        })
        .collect::<HashSet<_>>();
    assert_eq!(
        timeout_qcs.len(),
        1,
        "multiple timeout_qcs found at {:?}: {:?}",
        view,
        timeout_qcs
    );

    let timeout_qc = timeout_qcs
        .iter()
        .next()
        .unwrap()
        .clone()
        .expect("collected timeout_qc shouldn't be None");

    // NOTE: This check could be failed if other timeout_qcs had occurred
    //       before `consensus_infos` were gathered.
    //       But it should be okay as long as the `timeout` is not too short.
    assert_eq!(timeout_qc.view(), view);

    timeout_qc
}

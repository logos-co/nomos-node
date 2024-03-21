use futures::{stream, StreamExt};
use nomos_core::block::Block;
use nomos_node::Tx;
use std::{collections::HashSet, net::SocketAddr, path::PathBuf, time::Duration};
use tests::{get_available_port, MixNode, Node, NomosNode, SpawnConfig};

fn run_explorer(explorer_api_addr: SocketAddr, db_path: PathBuf) {
    let cfg = nomos_explorer::Config {
        log: Default::default(),
        api: nomos_api::ApiServiceSettings {
            backend_settings: nomos_explorer::AxumBackendSettings {
                address: explorer_api_addr,
                cors_origins: Vec::new(),
            },
        },
        storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
            db_path,
            read_only: true,
            column_family: None,
        },
    };
    std::thread::spawn(move || nomos_explorer::Explorer::run(cfg).unwrap());
}

#[tokio::test]
async fn explorer_depth() {
    let (_mixnodes, mixnet_config) = MixNode::spawn_nodes(3).await;
    let nodes = NomosNode::spawn_nodes(SpawnConfig::chain_happy(2, mixnet_config), None).await;
    let explorer_db = nodes[0].config().storage.db_path.clone();

    // wait for nodes to create blocks
    let explorer_api_addr: SocketAddr = format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap();
    let explorer_api_addr1: reqwest::Url =
        format!("http://{}/explorer/blocks/depth", explorer_api_addr)
            .parse()
            .unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                run_explorer(explorer_api_addr, explorer_db);
            });
    });
    tokio::time::sleep(Duration::from_secs(5)).await;

    let infos = stream::iter(nodes.iter())
        .then(|n| async move { n.get_blocks_info(None, None).await })
        .collect::<Vec<_>>()
        .await;
    let blocks = infos
        .iter()
        .map(|i| i.iter().find(|b| b.view == 20.into()).unwrap())
        .collect::<HashSet<_>>();

    for block in blocks.iter() {
        let resp = reqwest::Client::new()
            .get(explorer_api_addr1.clone())
            .query(&[("from", block.id.to_string()), ("depth", 10.to_string())])
            .header("Content-Type", "application/json")
            .send()
            .await
            .unwrap()
            .json::<Vec<Block<Tx, full_replication::Certificate>>>()
            .await
            .unwrap();
        assert_eq!(resp.len(), 10);
    }
}

#[tokio::test]
async fn explorer() {
    let (_mixnodes, mixnet_config) = MixNode::spawn_nodes(3).await;
    let nodes = NomosNode::spawn_nodes(SpawnConfig::chain_happy(2, mixnet_config), None).await;
    let explorer_db = nodes[0].config().storage.db_path.clone();

    // wait for nodes to create blocks
    let explorer_api_addr: SocketAddr = format!("127.0.0.1:{}", get_available_port())
        .parse()
        .unwrap();
    let explorer_api_addr1: reqwest::Url = format!("http://{}/explorer/blocks", explorer_api_addr)
        .parse()
        .unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                run_explorer(explorer_api_addr, explorer_db);
            });
    });
    tokio::time::sleep(Duration::from_secs(5)).await;

    let infos = stream::iter(nodes.iter())
        .then(|n| async move { n.get_blocks_info(None, None).await })
        .collect::<Vec<_>>()
        .await;
    let blocks = infos
        .iter()
        .map(|i| i.iter().find(|b| b.view == 20.into()).unwrap())
        .collect::<HashSet<_>>();
    for block in blocks.iter() {
        let from = block.parent().to_string();
        let to = block.id.to_string();

        let resp = reqwest::Client::new()
            .get(explorer_api_addr1.clone())
            .query(&[("from", from), ("to", to)])
            .header("Content-Type", "application/json")
            .send()
            .await
            .unwrap()
            .json::<Vec<Block<Tx, full_replication::Certificate>>>()
            .await
            .unwrap();
        assert!(!resp.is_empty());
    }
}

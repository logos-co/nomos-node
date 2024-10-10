use executor_http_client::ExecutorHttpClient;

use reqwest::ClientBuilder;
use reqwest::Url;
use std::time::Duration;
use tests::nodes::nomos::{NetworkNode, NetworkNodeConfig};
use tests::Node;
use tests::SpawnConfig;

const APP_ID: &str = "fd3384e132ad02a56c78f45547ee40038dc79002b90d29ed90e08eee762ae715";

async fn disseminate(nodes: &Vec<NetworkNode>) {
    // Nomos Cli is acting as the first node when dispersing the data by using the key associated
    // with that Nomos Node.
    let first_config = nodes[0].config();

    let client = ClientBuilder::new()
        .build()
        .expect("Client from default settings should be able to build");

    let backend_address = match first_config {
        NetworkNodeConfig::Validator(cfg) => cfg.http.backend_settings.address,
        NetworkNodeConfig::Executor(cfg) => cfg.http.backend_settings.address,
    };
    let exec_url = Url::parse(&format!("http://{}", backend_address)).unwrap();
    let client = ExecutorHttpClient::new(client, exec_url);

    let data = [1u8; 31];

    let app_id = hex::decode(APP_ID).unwrap();
    let metadata = kzgrs_backend::dispersal::Metadata::new(app_id.try_into().unwrap(), 0u64.into());
    client.publish_blob(data.to_vec(), metadata).await.unwrap();
}

#[tokio::test]
async fn disseminate_and_retrieve() {
    let nodes = NetworkNode::spawn_nodes(SpawnConfig::star_happy(
        2,
        1,
        tests::DaConfig {
            dispersal_factor: 2,
            subnetwork_size: 2,
            num_subnets: 2,
            ..Default::default()
        },
    ))
    .await;

    tokio::time::sleep(Duration::from_secs(15)).await;
    disseminate(&nodes).await;
    tokio::time::sleep(Duration::from_secs(20)).await;

    let from = 0u64.to_be_bytes();
    let to = 1u64.to_be_bytes();
    let app_id = hex::decode(APP_ID).unwrap();

    let node1_blobs = nodes[0]
        .get_indexer_range(app_id.clone().try_into().unwrap(), from..to)
        .await;
    let node2_blobs = nodes[1]
        .get_indexer_range(app_id.try_into().unwrap(), from..to)
        .await;

    let node1_idx_0_blobs: Vec<_> = node1_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs)
        .collect();
    let node2_idx_0_blobs: Vec<_> = node2_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs)
        .collect();

    // Index zero shouldn't be empty, node 2 replicated both blobs to node 1 because they both
    // are in the same subnetwork.)
    println!(">>> first node: {node1_idx_0_blobs:?}");
    println!(">>> second node: {node2_idx_0_blobs:?}");
    assert!(node1_idx_0_blobs.len() == 2);
    assert!(node2_idx_0_blobs.len() == 2);
    for b in node1_idx_0_blobs.iter() {
        assert!(!b.is_empty())
    }

    for b in node2_idx_0_blobs.iter() {
        assert!(!b.is_empty())
    }
}
